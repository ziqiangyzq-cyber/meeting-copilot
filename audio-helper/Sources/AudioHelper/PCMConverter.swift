import Foundation
import AVFoundation

// Converts incoming PCM (any format) to 16kHz mono int16 LE PCM Data
class PCMConverter {
    private var converter: AVAudioConverter?
    private let targetFormat: AVAudioFormat

    init() {
        targetFormat = AVAudioFormat(commonFormat: .pcmFormatInt16,
                                     sampleRate: 16000,
                                     channels: 1,
                                     interleaved: true)!
    }

    func extractPCM(from sampleBuffer: CMSampleBuffer) -> Data? {
        guard let formatDesc = CMSampleBufferGetFormatDescription(sampleBuffer) else {
            return nil
        }
        guard let asbdPtr = CMAudioFormatDescriptionGetStreamBasicDescription(formatDesc) else {
            return nil
        }
        var asbd = asbdPtr.pointee

        guard let inputFormat = AVAudioFormat(streamDescription: &asbd) else {
            return nil
        }

        let numSamples = CMSampleBufferGetNumSamples(sampleBuffer)
        guard let pcmBuffer = AVAudioPCMBuffer(pcmFormat: inputFormat,
                                               frameCapacity: AVAudioFrameCount(numSamples))
        else { return nil }
        pcmBuffer.frameLength = pcmBuffer.frameCapacity

        let status = CMSampleBufferCopyPCMDataIntoAudioBufferList(
            sampleBuffer, at: 0,
            frameCount: Int32(pcmBuffer.frameLength),
            into: pcmBuffer.mutableAudioBufferList
        )
        guard status == noErr else { return nil }

        return convert(pcmBuffer)
    }

    func convert(_ inputBuffer: AVAudioPCMBuffer) -> Data? {
        let inputFormat = inputBuffer.format
        if converter == nil || converter?.inputFormat != inputFormat {
            converter = AVAudioConverter(from: inputFormat, to: targetFormat)
        }
        guard let converter = converter else { return nil }

        let ratio = targetFormat.sampleRate / inputFormat.sampleRate
        let outCapacity = AVAudioFrameCount(Double(inputBuffer.frameLength) * ratio) + 32
        guard let outBuffer = AVAudioPCMBuffer(pcmFormat: targetFormat,
                                               frameCapacity: outCapacity)
        else { return nil }

        var error: NSError?
        let inputBlock: AVAudioConverterInputBlock = { _, outStatus in
            outStatus.pointee = .haveData
            return inputBuffer
        }
        converter.convert(to: outBuffer, error: &error, withInputFrom: inputBlock)
        if let error = error {
            logError("converter error: \(error)")
            return nil
        }

        let byteCount = Int(outBuffer.frameLength) * 2  // int16 = 2 bytes
        guard let int16Ptr = outBuffer.int16ChannelData?[0] else { return nil }
        return Data(bytes: int16Ptr, count: byteCount)
    }
}
