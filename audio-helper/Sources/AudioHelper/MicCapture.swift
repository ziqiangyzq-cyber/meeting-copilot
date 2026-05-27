import Foundation
import AVFoundation

class MicCapture {
    private var engine = AVAudioEngine()  // var: gets replaced on hot-swap
    private let converter: PCMConverter
    private var isRunning = false
    private var notifObserver: NSObjectProtocol?
    private var restartScheduled = false

    init(converter: PCMConverter) {
        self.converter = converter
    }

    func start() throws {
        try installTapAndStart()
        // Listen for ANY audio engine config change in this process (object: nil).
        // We only have one engine here; using nil keeps the subscription valid
        // even after we replace `self.engine` on hot-swap.
        notifObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange,
            object: nil,
            queue: nil
        ) { [weak self] _ in
            self?.scheduleRestart()
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

    /// Coalesce rapid-fire config changes (macOS sometimes fires 2-3 in quick succession during a plug event).
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
        logInfo("audio config changed — recreating engine on new default device")

        // Tear down old engine completely
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        engine.reset()

        // Replace with a fresh instance so internal state is clean
        engine = AVAudioEngine()

        do {
            try installTapAndStart()
        } catch {
            logError("mic restart after device change failed: \(error)")
        }
    }

    /// Manual trigger — for when the auto observer fails to detect a change
    /// (rare on macOS but worth having as a fallback).
    func manualRestart() {
        logInfo("manual mic restart requested")
        performRestart()
    }

    func stop() {
        if let obs = notifObserver {
            NotificationCenter.default.removeObserver(obs)
            notifObserver = nil
        }
        if isRunning {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            engine.reset()
            isRunning = false
            logInfo("mic capture stopped")
        }
    }
}
