import Foundation
import AVFoundation
import CoreAudio

class MicCapture {
    private var engine = AVAudioEngine()
    private let converter: PCMConverter
    private var isRunning = false
    private var restartScheduled = false
    private var coreAudioListenerInstalled = false

    init(converter: PCMConverter) {
        self.converter = converter
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

        // 2. Install tap + start
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
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultInputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        let status = AudioObjectAddPropertyListenerBlock(
            AudioObjectID(kAudioObjectSystemObject),
            &address,
            DispatchQueue.main
        ) { [weak self] _, _ in
            logInfo("core audio default input device changed")
            self?.scheduleRestart()
        }
        if status == noErr {
            coreAudioListenerInstalled = true
            logInfo("installed core audio default-input listener")
        } else {
            logError("failed to install core audio listener: OSStatus=\(status)")
        }
    }

    /// Coalesce rapid-fire change events (macOS often fires 2-3 per real plug event).
    private func scheduleRestart() {
        if restartScheduled { return }
        restartScheduled = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { [weak self] in
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

        // New engine instance ensures clean state
        engine = AVAudioEngine()

        do {
            try buildEngineAndStart()
        } catch {
            logError("mic restart after device change failed: \(error)")
        }
    }

    /// Manual trigger — fallback for when the core-audio observer somehow misses.
    func manualRestart() {
        logInfo("manual mic restart requested")
        performRestart()
    }

    func stop() {
        // Note: the core-audio listener block is intentionally NOT removed.
        // The process will exit shortly after stop() anyway, and removing a Block
        // listener requires holding the original Block reference which is awkward.
        // Process exit cleans it up.
        if isRunning {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            engine.reset()
            isRunning = false
            logInfo("mic capture stopped")
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
        // engine.inputNode.audioUnit returns AudioUnit? — must touch inputNode at least once
        // to instantiate the underlying AUHAL.
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
