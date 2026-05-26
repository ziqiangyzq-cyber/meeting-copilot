import Foundation
import AVFoundation

class MicCapture {
    private let engine = AVAudioEngine()
    private let converter: PCMConverter
    private var isRunning = false

    init(converter: PCMConverter) {
        self.converter = converter
    }

    func start() throws {
        // Microphone permission will be requested via NSMicrophoneUsageDescription on first use
        let input = engine.inputNode
        let inputFormat = input.outputFormat(forBus: 0)

        input.installTap(onBus: 0, bufferSize: 1024, format: inputFormat) { [weak self] buffer, _ in
            guard let self = self else { return }
            guard let pcmData = self.converter.convert(buffer) else { return }
            writeFrame(source: .mic, pcm: pcmData, to: FileHandle.standardOutput)
        }

        try engine.start()
        isRunning = true
        logInfo("mic capture started, input format: \(inputFormat)")
    }

    func stop() {
        if isRunning {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            isRunning = false
            logInfo("mic capture stopped")
        }
    }
}
