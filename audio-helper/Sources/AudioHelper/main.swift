import Foundation

logInfo("AudioHelper started")

// Separate converters per stream: AVAudioConverter caches per-input-format
// resampler state, and the mic/system formats differ — sharing one instance
// caused a converter rebuild on nearly every buffer (state reset = boundary
// artifacts in the 16k stream fed to ASR) plus a cross-thread data race
// between the mic tap thread and the SCK output queue.
let micConverter = PCMConverter()
let systemConverter = PCMConverter()
var systemCapture: SystemAudioCapture?
let micCapture = MicCapture(converter: micConverter)

if #available(macOS 13.0, *) {
    systemCapture = SystemAudioCapture(converter: systemConverter)
} else {
    logError("macOS 13.0+ required for ScreenCaptureKit")
    exit(1)
}

func handleCommand(_ cmd: Command) async {
    switch cmd.cmd {
    case "start":
        // Honor voice_processing flag from the start command (default true if missing).
        // The two captures start independently — one failing (e.g. Bluetooth mic
        // mid-handshake at meeting start) must not prevent the other, and each
        // class self-retries on initial failure.
        micCapture.setVoiceProcessingEnabled(cmd.voice_processing ?? true)
        micCapture.setLockBuiltinMic(cmd.lock_builtin_mic ?? false)
        await systemCapture?.start()
        micCapture.start()
    case "stop":
        do {
            try await systemCapture?.stop()
            micCapture.stop()
        } catch {
            logError("stop failed: \(error)")
        }
        exit(0)
    case "ping":
        logInfo("pong")
    case "restart_mic":
        micCapture.manualRestart()
    case "restart_system":
        await systemCapture?.restart()
    case "set_mic_enabled":
        let enabled = cmd.mic_enabled ?? true
        logInfo("set_mic_enabled: \(enabled)")
        micCapture.setEnabled(enabled)
    case "set_voice_processing":
        let enabled = cmd.voice_processing ?? true
        logInfo("set_voice_processing: \(enabled) (will restart mic)")
        micCapture.setVoiceProcessingEnabled(enabled)
        micCapture.manualRestart()
    case "set_lock_builtin_mic":
        let enabled = cmd.lock_builtin_mic ?? false
        logInfo("set_lock_builtin_mic: \(enabled) (will restart mic)")
        micCapture.setLockBuiltinMic(enabled)
        micCapture.manualRestart()
    default:
        logError("unknown command: \(cmd.cmd)")
    }
}

let semaphore = DispatchSemaphore(value: 0)

DispatchQueue.global().async {
    while let line = readLine() {
        guard let data = line.data(using: .utf8) else {
            logError("non-utf8 input")
            continue
        }
        do {
            let cmd = try JSONDecoder().decode(Command.self, from: data)
            Task { await handleCommand(cmd) }
        } catch {
            logError("decode failed: \(error)")
        }
    }
    semaphore.signal()
}

semaphore.wait()
logInfo("AudioHelper exiting (stdin closed)")
