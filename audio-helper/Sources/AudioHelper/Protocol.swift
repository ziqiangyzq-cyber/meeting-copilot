import Foundation

// stdin: line-delimited JSON commands
struct Command: Codable {
    let cmd: String
    // extensible fields
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

func writeFrame(source: AudioSource, pcm: Data, to fd: FileHandle) {
    var magic = frameMagic.littleEndian
    var src = source.rawValue.littleEndian
    var size = UInt32(pcm.count).littleEndian
    fd.write(Data(bytes: &magic, count: 4))
    fd.write(Data(bytes: &src, count: 4))
    fd.write(Data(bytes: &size, count: 4))
    fd.write(pcm)
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
