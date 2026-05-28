import Foundation

// stdin: line-delimited JSON commands
struct Command: Codable {
    let cmd: String
    let voice_processing: Bool?  // optional, only meaningful for "start"
}

// stdout: binary PCM frames
// Frame format:
//   [4 bytes magic = 0xAB12CD34]
//   [4 bytes source_tag: 0=system, 1=mic]
//   [4 bytes frame_size in bytes (little-endian uint32)]
//   [frame_size bytes PCM int16 16kHz mono LE]
enum AudioSource: UInt32 {
    case system = 0
    case mic = 1
}

let frameMagic: UInt32 = 0xAB12CD34

// Serial queue for stdout writes (prevents frame corruption when multiple capture queues write concurrently)
private let stdoutWriteQueue = DispatchQueue(label: "stdout-write-serializer")

func writeFrame(source: AudioSource, pcm: Data, to fd: FileHandle) {
    stdoutWriteQueue.sync {
        var magic = frameMagic.littleEndian
        var src = source.rawValue.littleEndian
        var size = UInt32(pcm.count).littleEndian
        // Pack into single Data first to minimize write calls (still atomic via serial queue)
        var packet = Data(capacity: 12 + pcm.count)
        packet.append(Data(bytes: &magic, count: 4))
        packet.append(Data(bytes: &src, count: 4))
        packet.append(Data(bytes: &size, count: 4))
        packet.append(pcm)
        fd.write(packet)
    }
}

// stderr: JSON log lines
func logInfo(_ msg: String) {
    let line = "{\"level\":\"info\",\"msg\":\"\(msg)\"}\n"
    FileHandle.standardError.write(line.data(using: .utf8)!)
}

func logError(_ msg: String) {
    let line = "{\"level\":\"error\",\"msg\":\"\(msg)\"}\n"
    FileHandle.standardError.write(line.data(using: .utf8)!)
}
