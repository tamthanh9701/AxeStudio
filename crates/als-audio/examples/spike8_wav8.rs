//! Spike S-08 (issue #7): 8 track WAV đồng bộ qua WASAPI — mục tiêu 0 xrun
//! trong 30 phút ở buffer 512 @48kHz, lặp lại cho 256/1024.
//!
//! Chạy trên MÁY ĐO Windows (kết quả điền vào docs/phase0/spike-report.md):
//!
//! ```text
//! // Bước 1 — sinh 8 file WAV thử (35 phút/file, đủ dài hơn lần phát;
//! //            CẤM dùng file ngắn lặp lại dưới 30 phút theo issue #7):
//! cargo run -p als-audio --release --example spike8_wav8 -- --gen-input .\spike8-input --minutes 30
//!
//! // Bước 2 — phát 30 phút, đọc bộ đếm xrun:
//! cargo run -p als-audio --release --example spike8_wav8 -- --buffer 512 --minutes 30 .\spike8-input\t01.wav ... .\spike8-input\t08.wav
//! ```
//!
//! Đầu ra dòng cuối: `RESULT,S-08,<buffer>,<minutes>,<xrun>` — dán thẳng vào
//! bảng S-08 của spike report. Lặp với --buffer 256 và 1024.

use als_audio::{AudioConfig, BufferSource, EngineBuilder};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SR: u32 = 48_000;
/// 8 tần số khác nhau để nghe được lệch nhịp bằng tai nếu có.
const FREQS_HZ: [f64; 8] = [
    220.00, 246.94, 277.18, 329.63, 392.00, 440.00, 493.88, 554.37,
];

fn main() {
    let mut wavs: Vec<PathBuf> = Vec::new();
    let mut buffer: u32 = 512;
    let mut minutes: u64 = 30;
    let mut gen_dir: Option<PathBuf> = None;

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--buffer" => {
                buffer = it
                    .next()
                    .expect("--buffer cần giá trị")
                    .parse()
                    .expect("--buffer phải là số");
            }
            "--minutes" => {
                minutes = it
                    .next()
                    .expect("--minutes cần giá trị")
                    .parse()
                    .expect("--minutes phải là số");
            }
            "--gen-input" => {
                gen_dir = Some(PathBuf::from(it.next().expect("--gen-input cần thư mục")));
            }
            other => wavs.push(PathBuf::from(other)),
        }
    }

    if !matches!(buffer, 256 | 512 | 1024) {
        panic!("--buffer chỉ nhận 256|512|1024 (theo spike report), nhận: {buffer}");
    }

    if let Some(dir) = gen_dir {
        gen_inputs(&dir, minutes);
        return;
    }

    if wavs.len() != 8 {
        eprintln!("Cần đúng 8 file WAV (8 track). Nhận: {}", wavs.len());
        eprintln!(
            "Usage: spike8_wav8 [--buffer 256|512|1024] [--minutes N] <t01.wav> ... <t08.wav>"
        );
        eprintln!("       spike8_wav8 --gen-input <dir> --minutes 30");
        std::process::exit(2);
    }

    let mut builder = EngineBuilder::new(AudioConfig {
        sample_rate: SR,
        buffer_size: buffer,
    });
    for p in &wavs {
        builder = builder.with_source(Box::new(load_wav(p)));
    }
    let mut engine = builder.start().expect("mở thiết bị output thất bại");
    let xruns = engine.xruns();

    println!(
        "Phát {} track @48kHz, buffer={buffer}, trong {minutes} phút. Ctrl-C để dừng sớm.",
        wavs.len()
    );
    engine.play();
    let start = Instant::now();
    let total = Duration::from_secs(minutes * 60);
    let mut next_report_min = 1u64;
    while start.elapsed() < total {
        // Sleep ở CONTROL thread (main), không phải audio callback — luật
        // clippy.toml cấm thread::sleep để bảo vệ callback; ở đây là vòng chờ
        // báo cáo của tool đo, không có đường nào chạm callback.
        #[allow(clippy::disallowed_methods)]
        std::thread::sleep(Duration::from_secs(1));
        let elapsed_min = start.elapsed().as_secs() / 60;
        if elapsed_min >= next_report_min {
            next_report_min += 1;
            println!(
                "[{:>3} min] playhead = {:.1}s, xrun = {}",
                elapsed_min,
                engine.playhead().load_ms(SR) / 1000.0,
                xruns.load()
            );
        }
    }
    engine.pause();
    println!("RESULT,S-08,{buffer},{minutes},{},", xruns.load());
}

/// Sinh 8 sine WAV 48kHz stereo 16-bit, dài `minutes + 5` phút — dư 5 phút để
/// source không bao giờ cạn giữa chừng (cạn đúng lúc sẽ đếm underrun là HỢP LỆ).
fn gen_inputs(dir: &Path, minutes: u64) {
    std::fs::create_dir_all(dir).expect("tạo thư mục input thất bại");
    let total_frames = ((minutes + 5) as usize) * 60 * SR as usize;
    for (i, &f) in FREQS_HZ.iter().enumerate() {
        let path = dir.join(format!("t{:02}.wav", i + 1));
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: SR,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).expect("tạo file WAV thất bại");
        for frame in 0..total_frames {
            let t = frame as f64 / f64::from(SR);
            // Biên độ ~0.18 full-scale mỗi track; mixer clamp master nên cộng
            // dồn 8 track vẫn nằm trong [-1, 1].
            let v = (0.18 * (2.0 * std::f64::consts::PI * f * t).sin() * 32767.0) as i16;
            w.write_sample(v).expect("ghi sample thất bại");
            w.write_sample(v).expect("ghi sample thất bại");
        }
        w.finalize().expect("finalize WAV thất bại");
        println!("đã ghi {} ({total_frames} frames)", path.display());
    }
}

/// Decode WAV 48kHz stereo 16-bit → BufferSource (interleaved f32).
fn load_wav(path: &Path) -> BufferSource {
    let mut reader = hound::WavReader::open(path).expect("mở WAV thất bại");
    let spec = reader.spec();
    assert_eq!(
        spec.sample_rate,
        SR,
        "{}: sample rate phải 48000",
        path.display()
    );
    assert_eq!(spec.channels, 2, "{}: phải stereo", path.display());
    assert_eq!(
        spec.sample_format,
        hound::SampleFormat::Int,
        "{}: phải PCM 16-bit (file do --gen-input sinh ra)",
        path.display()
    );
    let data: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| f32::from(s.expect("đọc sample thất bại")) / 32768.0)
        .collect();
    let frames = data.len() / 2;
    println!(
        "{}: {frames} frames ({:.1} phút)",
        path.display(),
        frames as f64 / 60.0 / f64::from(SR)
    );
    BufferSource::from_interleaved(data)
}
