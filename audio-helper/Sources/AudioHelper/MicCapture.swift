import Foundation
import AVFoundation
import CoreAudio

class MicCapture {
    private var engine = AVAudioEngine()
    private let converter: PCMConverter
    private var isRunning = false
    private var restartScheduled = false
    private var coreAudioListenerInstalled = false
    private var voiceProcessingEnabled: Bool = true  // default ON, overridable
    private var configChangeObserver: NSObjectProtocol?
    private var lockBuiltinMic = false  // pin capture to built-in mic; default follow system
    // Retry-with-backoff for failed (re)starts — Bluetooth transitions routinely
    // leave the device unready for a second or two; one-shot restarts used to
    // leave the mic dead until external recovery kicked in.
    private var failRetryAttempts = 0
    private let maxFailRetries = 3

    /// Serial queue for all mic operations. We can't use DispatchQueue.main because
    /// main.swift blocks the main thread on a semaphore (keeps the process alive
    /// while reading stdin), so main queue blocks never execute.
    private let micQueue = DispatchQueue(label: "meeting-copilot.mic-control")

    init(converter: PCMConverter) {
        self.converter = converter
    }

    /// Set whether to use macOS built-in voice processing (echo cancel + noise suppress + AGC).
    /// Call before start() or apply on next restart.
    func setVoiceProcessingEnabled(_ enabled: Bool) {
        self.voiceProcessingEnabled = enabled
    }

    /// Pin capture to the built-in microphone instead of following the system
    /// default input. AirPods on/off then never touches the capture path.
    func setLockBuiltinMic(_ enabled: Bool) {
        self.lockBuiltinMic = enabled
    }

    /// Start capture. Never throws: if the device isn't ready (e.g. meeting
    /// started mid-Bluetooth-handshake), retries are scheduled instead of the
    /// mic staying dead for the whole meeting.
    func start() {
        isRunning = true
        installCoreAudioListener()
        do {
            try buildEngineAndStart()
        } catch {
            logError("mic initial start failed: \(error) — scheduling retries")
            manualRestart()
        }
    }

    private func buildEngineAndStart() throws {
        // 1. Set the input device EXPLICITLY: built-in mic when locked
        //    (falling back to default if it can't be resolved), else the
        //    current system default.
        let chosenID: AudioDeviceID?
        if lockBuiltinMic {
            if let builtin = builtinInputDeviceID() {
                chosenID = builtin
            } else {
                logError("locked to built-in mic but none found — falling back to system default")
                chosenID = currentDefaultInputDeviceID()
            }
        } else {
            chosenID = currentDefaultInputDeviceID()
        }
        if let deviceID = chosenID {
            let name = deviceName(for: deviceID) ?? "unknown"
            logInfo("setting mic to device: \(name) (id=\(deviceID))\(lockBuiltinMic ? " [locked to built-in]" : "")")
            setInputDevice(on: engine, deviceID: deviceID)
        } else {
            logError("could not get input device id, falling back to engine default")
        }

        // 2. Enable voice processing (echo cancel + noise suppress + AGC) on inputNode
        //    BEFORE installing the tap. Must touch inputNode AFTER setting device.
        //
        //    Gate: only engage VPIO when meeting audio plays through the BUILT-IN
        //    speakers (外放). With headphones — especially Bluetooth — there is no
        //    acoustic echo to cancel, and turning VPIO on forces Bluetooth output from
        //    high-quality A2DP down to call-mode HFP/SCO: playback volume craters and
        //    mic capture frequently breaks. So off-speaker we skip VPIO regardless of
        //    the user toggle. The manual toggle can only turn it OFF, never force it on
        //    where it would break the mic.
        let useVoiceProcessing = voiceProcessingEnabled && outputIsBuiltInSpeaker()
        if useVoiceProcessing {
            do {
                try engine.inputNode.setVoiceProcessingEnabled(true)
                logInfo("mic voice processing enabled (echo cancel + noise suppress + AGC)")
            } catch {
                logError("setVoiceProcessingEnabled failed: \(error) — continuing without voice processing")
            }
        } else if voiceProcessingEnabled {
            logInfo("mic voice processing requested but SKIPPED — output is not built-in speakers (headphones/Bluetooth detected; no echo to cancel, and VPIO would break Bluetooth mic)")
        } else {
            logInfo("mic voice processing DISABLED by user setting")
        }

        // 3. Install tap + start
        let input = engine.inputNode
        let inputFormat = input.outputFormat(forBus: 0)
        input.installTap(onBus: 0, bufferSize: 1024, format: inputFormat) { [weak self] buffer, _ in
            guard let self = self else { return }
            guard let pcmData = self.converter.convert(buffer) else { return }
            writeFrame(source: .mic, pcm: pcmData, to: FileHandle.standardOutput)
        }
        try engine.start()

        // 4. Watch for in-place engine config changes (e.g. AirPods switching
        // A2DP↔HFP profile: same device ID — the CoreAudio default-device
        // listeners never fire — but the engine's format changes and capture
        // can silently stall). Observer is per-engine-instance, so re-register
        // on every rebuild and drop the previous one.
        if let obs = configChangeObserver {
            NotificationCenter.default.removeObserver(obs)
        }
        configChangeObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange,
            object: engine,
            queue: nil
        ) { [weak self] _ in
            guard let self = self else { return }
            logInfo("mic engine configuration changed — scheduling restart")
            self.micQueue.async { self.scheduleRestart() }
        }

        logInfo("mic capture started, input format: \(inputFormat)")
    }

    private func installCoreAudioListener() {
        guard !coreAudioListenerInstalled else { return }
        // Use OUR serial queue, not main — main thread is permanently blocked
        // on a semaphore in main.swift, so DispatchQueue.main blocks never fire.

        // Watch default INPUT device (mic hot-swap) ...
        var inputAddr = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultInputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let inStatus = AudioObjectAddPropertyListenerBlock(
            AudioObjectID(kAudioObjectSystemObject), &inputAddr, micQueue
        ) { [weak self] _, _ in
            guard let self = self else { return }
            if self.lockBuiltinMic {
                // Locked: AirPods etc. taking over the system default input is
                // exactly the event we want to NOT react to.
                logInfo("core audio: default input changed — ignored (locked to built-in mic)")
                return
            }
            logInfo("core audio: default input device changed")
            self.scheduleRestart()
        }

        // ... and default OUTPUT device, because plugging in / removing headphones
        // changes whether VPIO should run (see outputIsBuiltInSpeaker()).
        var outputAddr = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let outStatus = AudioObjectAddPropertyListenerBlock(
            AudioObjectID(kAudioObjectSystemObject), &outputAddr, micQueue
        ) { [weak self] _, _ in
            logInfo("core audio: default output device changed (re-evaluating voice processing)")
            self?.scheduleRestart()
        }

        if inStatus == noErr || outStatus == noErr {
            coreAudioListenerInstalled = true
            logInfo("installed core audio default input/output listeners (on micQueue) in=\(inStatus) out=\(outStatus)")
        } else {
            logError("failed to install core audio listeners: in=\(inStatus) out=\(outStatus)")
        }
    }

    /// Coalesce rapid-fire change events.
    private func scheduleRestart() {
        // Caller is already on micQueue (or being routed to it via manualRestart's dispatch),
        // so direct field access is safe.
        if restartScheduled { return }
        restartScheduled = true
        micQueue.asyncAfter(deadline: .now() + 0.3) { [weak self] in
            self?.restartScheduled = false
            self?.performRestart()
        }
    }

    private func performRestart() {
        guard isRunning else { return }
        logInfo("restarting mic capture on new default device")

        // Tear down old engine completely
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        engine.reset()

        // Fresh instance ensures clean state
        engine = AVAudioEngine()

        do {
            try buildEngineAndStart()
            failRetryAttempts = 0
        } catch {
            failRetryAttempts += 1
            guard failRetryAttempts <= maxFailRetries else {
                logError("mic restart failed after \(maxFailRetries) retries: \(error) — giving up (external recovery may still retrigger)")
                failRetryAttempts = 0
                return
            }
            let delay = Double(failRetryAttempts) // 1s, 2s, 3s
            logError("mic restart failed: \(error) — retrying in \(delay)s (#\(failRetryAttempts))")
            micQueue.asyncAfter(deadline: .now() + delay) { [weak self] in
                self?.performRestart()
            }
        }
    }

    /// User-facing mic on/off toggle. OFF fully stops the engine (macOS orange mic
    /// indicator goes away); system audio capture is a separate process path and
    /// is not affected. While off, device-change restarts no-op (isRunning guard).
    func setEnabled(_ enabled: Bool) {
        micQueue.async { [weak self] in
            guard let self = self else { return }
            if enabled {
                guard !self.isRunning else { return }
                self.isRunning = true
                self.engine = AVAudioEngine()
                do {
                    try self.buildEngineAndStart()
                    logInfo("mic capture enabled by user toggle")
                } catch {
                    logError("mic enable failed: \(error) — scheduling retries")
                    self.scheduleRestart()
                }
            } else {
                guard self.isRunning else { return }
                self.engine.inputNode.removeTap(onBus: 0)
                self.engine.stop()
                self.engine.reset()
                self.isRunning = false
                logInfo("mic capture disabled by user toggle")
            }
        }
    }

    /// Manual trigger — invoked from stdin handler (which runs on a global queue).
    /// Hop to micQueue so we don't race with the listener callback or scheduled restarts.
    func manualRestart() {
        logInfo("manual mic restart requested")
        micQueue.async { [weak self] in
            self?.performRestart()
        }
    }

    func stop() {
        // The Core Audio listener block stays registered; the process is about to exit
        // anyway and removing a Block listener requires the original Block ref.
        micQueue.sync {
            if isRunning {
                engine.inputNode.removeTap(onBus: 0)
                engine.stop()
                engine.reset()
                isRunning = false
                logInfo("mic capture stopped")
            }
        }
    }

    // MARK: - Core Audio helpers

    private func currentDefaultInputDeviceID() -> AudioDeviceID? {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultInputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var deviceID: AudioDeviceID = kAudioObjectUnknown
        var size: UInt32 = UInt32(MemoryLayout<AudioDeviceID>.size)
        let status = AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject),
            &address, 0, nil, &size, &deviceID
        )
        if status == noErr && deviceID != kAudioObjectUnknown {
            return deviceID
        }
        return nil
    }

    private func currentDefaultOutputDeviceID() -> AudioDeviceID? {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var deviceID: AudioDeviceID = kAudioObjectUnknown
        var size: UInt32 = UInt32(MemoryLayout<AudioDeviceID>.size)
        let status = AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject),
            &address, 0, nil, &size, &deviceID
        )
        if status == noErr && deviceID != kAudioObjectUnknown {
            return deviceID
        }
        return nil
    }

    /// Find the built-in microphone: enumerate all audio devices, pick the one
    /// with built-in transport AND input streams (the built-in speakers are a
    /// separate built-in device with no inputs).
    private func builtinInputDeviceID() -> AudioDeviceID? {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDevices,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(
            AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size
        ) == noErr else { return nil }
        let count = Int(size) / MemoryLayout<AudioDeviceID>.size
        guard count > 0 else { return nil }
        var devices = [AudioDeviceID](repeating: kAudioObjectUnknown, count: count)
        guard AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size, &devices
        ) == noErr else { return nil }
        for dev in devices {
            guard transportType(for: dev) == kAudioDeviceTransportTypeBuiltIn else { continue }
            guard hasInputStreams(dev) else { continue }
            return dev
        }
        return nil
    }

    private func hasInputStreams(_ deviceID: AudioDeviceID) -> Bool {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyStreams,
            mScope: kAudioObjectPropertyScopeInput,
            mElement: kAudioObjectPropertyElementMain
        )
        var size: UInt32 = 0
        let status = AudioObjectGetPropertyDataSize(deviceID, &address, 0, nil, &size)
        return status == noErr && size > 0
    }

    private func transportType(for deviceID: AudioDeviceID) -> UInt32? {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyTransportType,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var transport: UInt32 = 0
        var size: UInt32 = UInt32(MemoryLayout<UInt32>.size)
        let status = AudioObjectGetPropertyData(deviceID, &address, 0, nil, &size, &transport)
        return status == noErr ? transport : nil
    }

    /// VPIO should only run when meeting audio comes out of the built-in speakers (外放),
    /// where the other party's voice can leak into the mic and needs echo cancellation.
    /// For Bluetooth / USB / any non-built-in output (i.e. headphones), there's no echo
    /// to cancel and VPIO does more harm than good — so we return false to skip it.
    /// When the output device or its transport can't be resolved we default to false
    /// (skip), since the worst failure (breaking the Bluetooth mic) is worse than the
    /// rare case of losing echo cancellation on speakers.
    private func outputIsBuiltInSpeaker() -> Bool {
        guard let outID = currentDefaultOutputDeviceID() else {
            logInfo("could not resolve default output device; skipping voice processing to be safe")
            return false
        }
        guard let transport = transportType(for: outID) else {
            logInfo("could not read output transport type; skipping voice processing to be safe")
            return false
        }
        let isBuiltIn = (transport == kAudioDeviceTransportTypeBuiltIn)
        let name = deviceName(for: outID) ?? "unknown"
        logInfo("default output: \(name) transport=\(transport) builtInSpeaker=\(isBuiltIn)")
        return isBuiltIn
    }

    private func deviceName(for deviceID: AudioDeviceID) -> String? {
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioObjectPropertyName,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        var name: Unmanaged<CFString>?
        var size: UInt32 = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        let status = AudioObjectGetPropertyData(deviceID, &address, 0, nil, &size, &name)
        guard status == noErr, let cf = name?.takeRetainedValue() else { return nil }
        return cf as String
    }

    private func setInputDevice(on engine: AVAudioEngine, deviceID: AudioDeviceID) {
        guard let audioUnit = engine.inputNode.audioUnit else {
            logError("inputNode.audioUnit is nil — cannot set device")
            return
        }
        var devID = deviceID
        let status = AudioUnitSetProperty(
            audioUnit,
            kAudioOutputUnitProperty_CurrentDevice,
            kAudioUnitScope_Global,
            0,
            &devID,
            UInt32(MemoryLayout<AudioDeviceID>.size)
        )
        if status != noErr {
            logError("AudioUnitSetProperty CurrentDevice failed: OSStatus=\(status)")
        }
    }
}
