import Foundation
import AVFoundation

class MicCapture {
    private let engine = AVAudioEngine()
    private let converter: PCMConverter
    private var isRunning = false
    private var notifObserver: NSObjectProtocol?

    init(converter: PCMConverter) {
        self.converter = converter
    }

    func start() throws {
        try installTapAndStart()
        // Listen for device / config changes (AirPods plug/unplug, default mic change, etc.)
        // When fired, tear down the current tap + engine and re-setup against the new default device.
        notifObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange,
            object: engine,
            queue: nil
        ) { [weak self] _ in
            self?.handleConfigChange()
        }
        isRunning = true
    }

    private func installTapAndStart() throws {
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

    private func handleConfigChange() {
        logInfo("audio configuration changed — restarting mic capture on new default device")
        // Remove old tap + stop engine
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        // Small delay so the OS finishes the device switch transition
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [weak self] in
            guard let self = self, self.isRunning else { return }
            do {
                try self.installTapAndStart()
            } catch {
                logError("mic restart after config change failed: \(error)")
            }
        }
    }

    func stop() {
        if let obs = notifObserver {
            NotificationCenter.default.removeObserver(obs)
            notifObserver = nil
        }
        if isRunning {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            isRunning = false
            logInfo("mic capture stopped")
        }
    }
}
