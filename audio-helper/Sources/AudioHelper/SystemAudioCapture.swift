import Foundation
import ScreenCaptureKit
import AVFoundation

@available(macOS 13.0, *)
class SystemAudioCapture: NSObject, SCStreamDelegate, SCStreamOutput {
    private var stream: SCStream?
    private let outputQueue = DispatchQueue(label: "system-audio-output")
    private let converter: PCMConverter
    private var isRunning = false
    private var restartAttempts = 0
    private let maxRestartAttempts = 5

    init(converter: PCMConverter) {
        self.converter = converter
        super.init()
    }

    /// Start capture. Never throws: initial failure (e.g. transient SCK/display
    /// state at meeting start) schedules background retries instead of leaving
    /// system audio dead for the whole meeting.
    func start() async {
        isRunning = true
        do {
            try await buildAndStart()
            restartAttempts = 0
        } catch {
            logError("system audio initial start failed: \(error) — scheduling retries")
            attemptRestart()
        }
    }

    private func buildAndStart() async throws {
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
        isRunning = false
        if let stream = stream {
            try await stream.stopCapture()
            self.stream = nil
            logInfo("system audio capture stopped")
        }
    }

    /// Manual restart, triggered from Rust (e.g. stall recovery). Tears down
    /// whatever is left and rebuilds the stream.
    func restart() async {
        logInfo("manual system audio restart requested")
        isRunning = true
        if let s = stream {
            try? await s.stopCapture()
            stream = nil
        }
        do {
            try await buildAndStart()
            restartAttempts = 0
            logInfo("system audio restarted (manual)")
        } catch {
            logError("manual system audio restart failed: \(error) — scheduling retries")
            attemptRestart()
        }
    }

    /// SCStream dying (display reconfiguration, permission hiccup, GPU reset)
    /// used to silently end system-audio capture for the rest of the meeting.
    /// Retry with linear backoff; give up after maxRestartAttempts.
    private func attemptRestart() {
        restartAttempts += 1
        guard restartAttempts <= maxRestartAttempts else {
            logError("system audio restart gave up after \(maxRestartAttempts) attempts")
            return
        }
        let delay = Double(restartAttempts) // 1s, 2s, ... 5s
        logInfo("attempting system audio restart #\(restartAttempts) in \(delay)s")
        Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
            guard let self = self, self.isRunning else { return }
            self.stream = nil
            do {
                try await self.buildAndStart()
                self.restartAttempts = 0
                logInfo("system audio restarted after error")
            } catch {
                logError("system audio restart failed: \(error)")
                self.attemptRestart()
            }
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
        guard isRunning else { return }
        attemptRestart()
    }
}
