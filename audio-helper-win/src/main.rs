// AudioHelper (Windows) — Rust + WASAPI parallel of the macOS Swift AudioHelper.
//
// Protocol (identical to macOS):
//   stdin  : line-delimited JSON commands ({"cmd":"start"} / {"cmd":"stop"} / {"cmd":"ping"})
//   stdout : binary frames
//            [4 bytes magic   = 0xAB12CD34 LE]
//            [4 bytes source  = 0 (system) | 1 (mic) LE]
//            [4 bytes payload = uint32 LE byte length]
//            [N bytes PCM int16 LE 16kHz mono]
//   stderr : JSON log lines ({"level":"info","msg":"..."} / {"level":"error",...})
//
// NOTE / Caveats (untested on real Windows hardware):
//   - Linear resampler. Cheap, OK for speech / ASR which downsamples to 16kHz anyway,
//     not audiophile-grade.
//   - WAVE_FORMAT_EXTENSIBLE detection (vs IEEE_FLOAT) is simplified. Most Windows 10/11
//     shared-mode mix formats are float32, but some hardware returns int16 in extensible
//     format. The `is_float` heuristic here is approximate. If a Windows tester reports
//     garbled audio, the proper fix is inspecting the SubFormat GUID on extensible.
//   - Capture loop uses a blocking pull (GetNextPacketSize + 5 ms sleep) instead of
//     event-driven WASAPI. Simpler, slightly higher CPU. Fine for an MVP.

use serde::Deserialize;
use std::io::BufRead;

#[cfg(target_os = "windows")]
use std::io::Write;
#[cfg(target_os = "windows")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use std::thread;

#[derive(Deserialize)]
struct Command {
    cmd: String,
}

#[cfg(target_os = "windows")]
const FRAME_MAGIC: u32 = 0xAB12_CD34;
#[cfg(target_os = "windows")]
const SOURCE_SYSTEM: u32 = 0;
#[cfg(target_os = "windows")]
const SOURCE_MIC: u32 = 1;
#[cfg(target_os = "windows")]
const TARGET_SR: u32 = 16000;

// Mutex-protected stdout for serialized frame writes (matches Swift behavior).
#[cfg(target_os = "windows")]
static STDOUT_LOCK: Mutex<()> = Mutex::new(());

fn log_info(msg: &str) {
    let line = format!(r#"{{"level":"info","msg":"{}"}}"#, msg.replace('"', "\\\""));
    eprintln!("{line}");
}

fn log_error(msg: &str) {
    let line = format!(r#"{{"level":"error","msg":"{}"}}"#, msg.replace('"', "\\\""));
    eprintln!("{line}");
}

#[cfg(target_os = "windows")]
fn write_frame(source: u32, pcm: &[u8]) {
    let _guard = STDOUT_LOCK.lock().unwrap();
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(&FRAME_MAGIC.to_le_bytes());
    let _ = stdout.write_all(&source.to_le_bytes());
    let _ = stdout.write_all(&(pcm.len() as u32).to_le_bytes());
    let _ = stdout.write_all(pcm);
    let _ = stdout.flush();
}

#[cfg(target_os = "windows")]
mod capture {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::core::Interface;
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::*;

    /// System audio capture (WASAPI loopback on default render device).
    pub struct SystemCapture {
        stop_flag: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<()>>,
    }

    impl SystemCapture {
        pub fn start() -> std::result::Result<Self, String> {
            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_clone = stop_flag.clone();

            let handle = thread::spawn(move || {
                unsafe {
                    let co_hr = CoInitializeEx(None, COINIT_MULTITHREADED);
                    if co_hr.is_err() {
                        log_error(&format!("system: CoInitializeEx failed: {co_hr:?}"));
                        return;
                    }

                    let enumerator: IMMDeviceEnumerator =
                        match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                            Ok(e) => e,
                            Err(e) => {
                                log_error(&format!("system: enumerator: {e:?}"));
                                CoUninitialize();
                                return;
                            }
                        };

                    // For loopback, we capture the RENDER endpoint (eRender + eConsole).
                    let device = match enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                        Ok(d) => d,
                        Err(e) => {
                            log_error(&format!("system: device: {e:?}"));
                            CoUninitialize();
                            return;
                        }
                    };

                    let client: IAudioClient = match device.Activate(CLSCTX_ALL, None) {
                        Ok(c) => c,
                        Err(e) => {
                            log_error(&format!("system: activate: {e:?}"));
                            CoUninitialize();
                            return;
                        }
                    };

                    let mix_format = match client.GetMixFormat() {
                        Ok(p) => p,
                        Err(e) => {
                            log_error(&format!("system: GetMixFormat: {e:?}"));
                            CoUninitialize();
                            return;
                        }
                    };

                    if let Err(e) = client.Initialize(
                        AUDCLNT_SHAREMODE_SHARED,
                        AUDCLNT_STREAMFLAGS_LOOPBACK,
                        10_000_000, // 1 sec buffer (REFERENCE_TIME = 100ns units)
                        0,
                        mix_format,
                        None,
                    ) {
                        log_error(&format!("system: Initialize: {e:?}"));
                        CoUninitialize();
                        return;
                    }

                    let capture_client: IAudioCaptureClient = match client.GetService() {
                        Ok(c) => c,
                        Err(e) => {
                            log_error(&format!("system: GetService: {e:?}"));
                            CoUninitialize();
                            return;
                        }
                    };

                    if let Err(e) = client.Start() {
                        log_error(&format!("system: Start: {e:?}"));
                        CoUninitialize();
                        return;
                    }

                    log_info("system audio capture started (WASAPI loopback)");

                    let source_format = mix_format.read();
                    let source_sr = source_format.nSamplesPerSec;
                    let source_channels = source_format.nChannels;
                    let bits_per_sample = source_format.wBitsPerSample;
                    let block_align = source_format.nBlockAlign;
                    // Approximate float detection. For WAVE_FORMAT_EXTENSIBLE we'd need
                    // to cast to WAVEFORMATEXTENSIBLE and inspect SubFormat GUID
                    // (KSDATAFORMAT_SUBTYPE_IEEE_FLOAT vs KSDATAFORMAT_SUBTYPE_PCM).
                    // Most shared-mode mix formats on Win10/11 are 32-bit float, so we
                    // assume float when wBitsPerSample == 32.
                    let is_float = source_format.wFormatTag == WAVE_FORMAT_IEEE_FLOAT as u16
                        || (source_format.wFormatTag == WAVE_FORMAT_EXTENSIBLE as u16
                            && bits_per_sample == 32);

                    let mut resampler =
                        Resampler::new(source_sr, TARGET_SR, source_channels as u32);

                    while !stop_clone.load(Ordering::Relaxed) {
                        let packet_size = match capture_client.GetNextPacketSize() {
                            Ok(s) => s,
                            Err(e) => {
                                log_error(&format!("system: GetNextPacketSize: {e:?}"));
                                break;
                            }
                        };
                        if packet_size == 0 {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                            continue;
                        }

                        let mut buffer: *mut u8 = std::ptr::null_mut();
                        let mut frames_available = 0u32;
                        let mut flags = 0u32;
                        if let Err(e) = capture_client.GetBuffer(
                            &mut buffer,
                            &mut frames_available,
                            &mut flags,
                            None,
                            None,
                        ) {
                            log_error(&format!("system: GetBuffer: {e:?}"));
                            break;
                        }

                        let byte_len = (frames_available as usize) * (block_align as usize);
                        if byte_len > 0 && !buffer.is_null() {
                            let slice = std::slice::from_raw_parts(buffer, byte_len);

                            let mono16 = if is_float && bits_per_sample == 32 {
                                convert_f32_to_mono_i16(
                                    slice,
                                    source_channels as usize,
                                    &mut resampler,
                                )
                            } else {
                                convert_i16_to_mono_i16(
                                    slice,
                                    source_channels as usize,
                                    &mut resampler,
                                )
                            };

                            if !mono16.is_empty() {
                                write_frame(SOURCE_SYSTEM, &pcm_to_bytes(&mono16));
                            }
                        }

                        if let Err(e) = capture_client.ReleaseBuffer(frames_available) {
                            log_error(&format!("system: ReleaseBuffer: {e:?}"));
                            break;
                        }
                    }

                    let _ = client.Stop();
                    log_info("system audio capture stopped");
                    CoUninitialize();
                }
            });

            Ok(Self {
                stop_flag,
                thread: Some(handle),
            })
        }
    }

    impl Drop for SystemCapture {
        fn drop(&mut self) {
            self.stop_flag.store(true, Ordering::Relaxed);
            if let Some(h) = self.thread.take() {
                let _ = h.join();
            }
        }
    }

    /// Microphone capture (WASAPI on default capture device).
    pub struct MicCapture {
        stop_flag: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<()>>,
    }

    impl MicCapture {
        pub fn start() -> std::result::Result<Self, String> {
            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_clone = stop_flag.clone();

            let handle = thread::spawn(move || {
                unsafe {
                    let co_hr = CoInitializeEx(None, COINIT_MULTITHREADED);
                    if co_hr.is_err() {
                        log_error(&format!("mic: CoInitializeEx failed: {co_hr:?}"));
                        return;
                    }

                    let enumerator: IMMDeviceEnumerator =
                        match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                            Ok(e) => e,
                            Err(e) => {
                                log_error(&format!("mic: enumerator: {e:?}"));
                                CoUninitialize();
                                return;
                            }
                        };

                    let device = match enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) {
                        Ok(d) => d,
                        Err(e) => {
                            log_error(&format!("mic: device: {e:?}"));
                            CoUninitialize();
                            return;
                        }
                    };

                    let client: IAudioClient = match device.Activate(CLSCTX_ALL, None) {
                        Ok(c) => c,
                        Err(e) => {
                            log_error(&format!("mic: activate: {e:?}"));
                            CoUninitialize();
                            return;
                        }
                    };

                    let mix_format = match client.GetMixFormat() {
                        Ok(p) => p,
                        Err(e) => {
                            log_error(&format!("mic: GetMixFormat: {e:?}"));
                            CoUninitialize();
                            return;
                        }
                    };

                    if let Err(e) = client.Initialize(
                        AUDCLNT_SHAREMODE_SHARED,
                        0,
                        10_000_000,
                        0,
                        mix_format,
                        None,
                    ) {
                        log_error(&format!("mic: Initialize: {e:?}"));
                        CoUninitialize();
                        return;
                    }

                    let capture_client: IAudioCaptureClient = match client.GetService() {
                        Ok(c) => c,
                        Err(e) => {
                            log_error(&format!("mic: GetService: {e:?}"));
                            CoUninitialize();
                            return;
                        }
                    };

                    if let Err(e) = client.Start() {
                        log_error(&format!("mic: Start: {e:?}"));
                        CoUninitialize();
                        return;
                    }

                    log_info("mic capture started (WASAPI)");

                    let source_format = mix_format.read();
                    let source_sr = source_format.nSamplesPerSec;
                    let source_channels = source_format.nChannels;
                    let bits_per_sample = source_format.wBitsPerSample;
                    let block_align = source_format.nBlockAlign;
                    let is_float = source_format.wFormatTag == WAVE_FORMAT_IEEE_FLOAT as u16
                        || (source_format.wFormatTag == WAVE_FORMAT_EXTENSIBLE as u16
                            && bits_per_sample == 32);

                    let mut resampler =
                        Resampler::new(source_sr, TARGET_SR, source_channels as u32);

                    while !stop_clone.load(Ordering::Relaxed) {
                        let packet_size = match capture_client.GetNextPacketSize() {
                            Ok(s) => s,
                            Err(e) => {
                                log_error(&format!("mic: GetNextPacketSize: {e:?}"));
                                break;
                            }
                        };
                        if packet_size == 0 {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                            continue;
                        }

                        let mut buffer: *mut u8 = std::ptr::null_mut();
                        let mut frames_available = 0u32;
                        let mut flags = 0u32;
                        if let Err(e) = capture_client.GetBuffer(
                            &mut buffer,
                            &mut frames_available,
                            &mut flags,
                            None,
                            None,
                        ) {
                            log_error(&format!("mic: GetBuffer: {e:?}"));
                            break;
                        }

                        let byte_len = (frames_available as usize) * (block_align as usize);
                        if byte_len > 0 && !buffer.is_null() {
                            let slice = std::slice::from_raw_parts(buffer, byte_len);

                            let mono16 = if is_float && bits_per_sample == 32 {
                                convert_f32_to_mono_i16(
                                    slice,
                                    source_channels as usize,
                                    &mut resampler,
                                )
                            } else {
                                convert_i16_to_mono_i16(
                                    slice,
                                    source_channels as usize,
                                    &mut resampler,
                                )
                            };

                            if !mono16.is_empty() {
                                write_frame(SOURCE_MIC, &pcm_to_bytes(&mono16));
                            }
                        }

                        if let Err(e) = capture_client.ReleaseBuffer(frames_available) {
                            log_error(&format!("mic: ReleaseBuffer: {e:?}"));
                            break;
                        }
                    }

                    let _ = client.Stop();
                    log_info("mic capture stopped");
                    CoUninitialize();
                }
            });

            Ok(Self {
                stop_flag,
                thread: Some(handle),
            })
        }
    }

    impl Drop for MicCapture {
        fn drop(&mut self) {
            self.stop_flag.store(true, Ordering::Relaxed);
            if let Some(h) = self.thread.take() {
                let _ = h.join();
            }
        }
    }

    /// Simple linear interpolation resampler (downsample-focused; OK for 44.1/48k → 16k).
    pub struct Resampler {
        in_sr: u32,
        out_sr: u32,
        #[allow(dead_code)]
        channels: u32,
        /// fractional position into the input stream (in input samples) — carry across calls
        pos: f64,
    }

    impl Resampler {
        pub fn new(in_sr: u32, out_sr: u32, channels: u32) -> Self {
            Self { in_sr, out_sr, channels, pos: 0.0 }
        }
    }

    pub fn convert_f32_to_mono_i16(
        raw: &[u8],
        channels: usize,
        resampler: &mut Resampler,
    ) -> Vec<i16> {
        let ch = channels.max(1);
        let sample_count = raw.len() / 4;
        let frame_count = sample_count / ch;
        let mut mono = Vec::with_capacity(frame_count);
        for frame in 0..frame_count {
            let mut sum = 0.0f32;
            for c in 0..ch {
                let idx = frame * ch + c;
                let bytes = &raw[idx * 4..idx * 4 + 4];
                let v = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                sum += v;
            }
            mono.push(sum / (ch as f32));
        }
        resample_linear_f32(&mono, resampler)
    }

    pub fn convert_i16_to_mono_i16(
        raw: &[u8],
        channels: usize,
        resampler: &mut Resampler,
    ) -> Vec<i16> {
        let ch = channels.max(1);
        let sample_count = raw.len() / 2;
        let frame_count = sample_count / ch;
        let mut mono = Vec::with_capacity(frame_count);
        for frame in 0..frame_count {
            let mut sum: i32 = 0;
            for c in 0..ch {
                let idx = frame * ch + c;
                let bytes = &raw[idx * 2..idx * 2 + 2];
                let v = i16::from_le_bytes([bytes[0], bytes[1]]) as i32;
                sum += v;
            }
            mono.push((sum / ch as i32) as f32 / 32768.0);
        }
        resample_linear_f32(&mono, resampler)
    }

    fn resample_linear_f32(input: &[f32], r: &mut Resampler) -> Vec<i16> {
        if input.is_empty() {
            return Vec::new();
        }
        if r.in_sr == r.out_sr {
            return input.iter().map(|&f| f32_to_i16(f)).collect();
        }
        let ratio = r.in_sr as f64 / r.out_sr as f64;
        let mut out = Vec::new();
        let mut pos = r.pos;
        let last_idx = input.len() as f64 - 1.0;
        while pos < last_idx {
            let i = pos as usize;
            let frac = pos - i as f64;
            let s = input[i] as f64 * (1.0 - frac) + input[i + 1] as f64 * frac;
            out.push(f32_to_i16(s as f32));
            pos += ratio;
        }
        // Save fractional position for next call (relative to the *next* buffer).
        r.pos = pos - input.len() as f64;
        if r.pos < 0.0 {
            r.pos = 0.0;
        }
        out
    }

    fn f32_to_i16(f: f32) -> i16 {
        let clamped = f.clamp(-1.0, 1.0);
        (clamped * 32767.0) as i16
    }

    // Suppress unused-import warning when Interface trait isn't directly referenced.
    #[allow(dead_code)]
    fn _force_interface_use<T: Interface>() {}
}

#[cfg(not(target_os = "windows"))]
mod capture {
    //! Non-Windows stub so `cargo check` passes on macOS / Linux.
    //! The real implementation is only compiled on Windows.
    pub struct SystemCapture;
    pub struct MicCapture;
    impl SystemCapture {
        pub fn start() -> Result<Self, String> {
            Err("system audio capture not supported on this OS (Windows only)".into())
        }
    }
    impl MicCapture {
        pub fn start() -> Result<Self, String> {
            Err("mic capture not supported on this OS (Windows only)".into())
        }
    }
}

#[cfg(target_os = "windows")]
fn pcm_to_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    bytes
}

fn main() {
    log_info("AudioHelper-win started");

    let mut sys_capture: Option<capture::SystemCapture> = None;
    let mut mic_capture: Option<capture::MicCapture> = None;

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { continue };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cmd = match serde_json::from_str::<Command>(trimmed) {
            Ok(c) => c,
            Err(e) => {
                log_error(&format!("decode failed: {e} (line: {trimmed})"));
                continue;
            }
        };
        match cmd.cmd.as_str() {
            "start" => {
                match capture::SystemCapture::start() {
                    Ok(s) => sys_capture = Some(s),
                    Err(e) => log_error(&format!("start sys failed: {e}")),
                }
                match capture::MicCapture::start() {
                    Ok(m) => mic_capture = Some(m),
                    Err(e) => log_error(&format!("start mic failed: {e}")),
                }
            }
            "stop" => {
                // Drop captures to signal their threads to stop + join.
                drop(sys_capture.take());
                drop(mic_capture.take());
                log_info("stopping");
                std::process::exit(0);
            }
            "ping" => log_info("pong"),
            other => log_error(&format!("unknown cmd: {other}")),
        }
    }

    // Stdin closed → drop captures (their Drop impls signal stop + join threads).
    drop(sys_capture);
    drop(mic_capture);
    log_info("AudioHelper exiting (stdin closed)");
}
