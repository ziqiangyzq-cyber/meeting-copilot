import Foundation

logInfo("AudioHelper started")

let converter = PCMConverter()
var systemCapture: SystemAudioCapture?
let micCapture = MicCapture(converter: converter)

if #available(macOS 13.0, *) {
    systemCapture = SystemAudioCapture(converter: converter)
} else {
    logError("macOS 13.0+ required for ScreenCaptureKit")
    exit(1)
}

func handleCommand(_ cmd: Command) async {
    switch cmd.cmd {
    case "start":
        do {
            // Honor voice_processing flag from the start command (default true if missing)
            micCapture.setVoiceProcessingEnabled(cmd.voice_processing ?? true)
            try await systemCapture?.start()
            try micCapture.start()
        } catch {
            logError("start failed: \(error)")
        }
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
    case "set_voice_processing":
        let enabled = cmd.voice_processing ?? true
        logInfo("set_voice_processing: \(enabled) (will restart mic)")
        micCapture.setVoiceProcessingEnabled(enabled)
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
