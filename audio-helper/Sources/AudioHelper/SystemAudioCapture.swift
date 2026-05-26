import Foundation
import ScreenCaptureKit
import AVFoundation

@available(macOS 13.0, *)
class SystemAudioCapture: NSObject, SCStreamDelegate, SCStreamOutput {
    private var stream: SCStream?
    private let outputQueue = DispatchQueue(label: "system-audio-output")
    private let converter: PCMConverter

    init(converter: PCMConverter) {
        self.converter = converter
        super.init()
    }

    func start() async throws {
        // Discover available content
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let display = content.displays.first else {
            throw NSError(domain: "SystemAudio", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: "no display found"])
        }

        let config = SCStreamConfiguration()
        config.capturesAudio = true
        config.excludesCurrentProcessAudio = true
        config.sampleRate = 16000
        config.channelCount = 1
        // Video portion must be enabled but use minimum settings
        config.width = 2
        config.height = 2
        config.minimumFrameInterval = CMTime(value: 1, timescale: 1)
        config.queueDepth = 8

        let filter = SCContentFilter(display: display, excludingWindows: [])

        let stream = SCStream(filter: filter, configuration: config, delegate: self)
        try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: outputQueue)
        try await stream.startCapture()
        self.stream = stream
        logInfo("system audio capture started")
    }

    func stop() async throws {
        if let stream = stream {
            try await stream.stopCapture()
            self.stream = nil
            logInfo("system audio capture stopped")
        }
    }

    // MARK: SCStreamOutput
    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio else { return }
        guard let pcmData = converter.extractPCM(from: sampleBuffer) else {
            return
        }
        writeFrame(source: .system, pcm: pcmData, to: FileHandle.standardOutput)
    }

    // MARK: SCStreamDelegate
    func stream(_ stream: SCStream, didStopWithError error: Error) {
        logError("system audio stream stopped with error: \(error)")
    }
}
