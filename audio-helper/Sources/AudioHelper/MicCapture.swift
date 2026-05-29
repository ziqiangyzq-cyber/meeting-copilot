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

    func start() throws {
        try buildEngineAndStart()
        installCoreAudioListener()
        isRunning = true
    }

    private func buildEngineAndStart() throws {
        // 1. Set the input device EXPLICITLY to the current default
        if let deviceID = currentDefaultInputDeviceID() {
            let name = deviceName(for: deviceID) ?? "unknown"
            logInfo("setting mic to device: \(name) (id=\(deviceID))")
            setInputDevice(on: engine, deviceID: deviceID)
        } else {
            logError("could not get default input device id, falling back to engine default")
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
            logInfo("core audio: default input device changed")
            self?.scheduleRestart()
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
        } catch {
            logError("mic restart after device change failed: \(error)")
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
