import Foundation

logInfo("AudioHelper started")

func handleCommand(_ cmd: Command) {
    switch cmd.cmd {
    case "start":
        logInfo("start command received (capture not implemented yet)")
        // Task 4-5 implement actual capture
    case "stop":
        logInfo("stop command received")
        exit(0)
    case "ping":
        logInfo("pong")
    default:
        logError("unknown command: \(cmd.cmd)")
    }
}

while let line = readLine() {
    guard let data = line.data(using: .utf8) else {
        logError("non-utf8 input")
        continue
    }
    do {
        let cmd = try JSONDecoder().decode(Command.self, from: data)
        handleCommand(cmd)
    } catch {
        logError("decode failed: \(error)")
    }
}

logInfo("AudioHelper exiting (stdin closed)")
