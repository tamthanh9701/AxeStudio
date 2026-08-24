//! Bench thời gian "callback" khi mixer kéo 8 track STREAMING @buffer 512
//! (ALS-B02 acceptance: < 1ms mỗi block — ngân sách `docs/perf-budget.md`
//! cho callback là 30% của ≈3.5ms).
//!
//! Chạy: `cargo bench -p als-audio` (release profile, harness=false).
//!
//! Nguồn audio in-RAM (giống tests/streaming.rs). Reader sẽ phát hết rồi
//! im lặng — cứ hết 30s nội dung thì seek về 0 ở control side (ngoài vùng
//! đo từng block) để giữ đường decode/refresh luôn nóng.
//!
//! `thread::sleep` ở đây là của bench/control thread — allow lint; crate
//! này không có (và không được có) tokio runtime.
#![allow(clippy::disallowed_methods)]

use std::io::Cursor;
use std::sync::Arc;
use std::time::{Duration, Instant};

use als_audio::source::AudioSource;
use als_audio::stream::open_source;
use als_audio::Mixer;

fn make_wav_bytes(seconds: u32, sample_rate: u32) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::<u8>::new());
    {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::new(&mut cursor, spec).expect("writer");
        for i in 0..seconds * sample_rate {
            let v = ((i as i64).wrapping_mul(2654435761) % 16001 - 8000) as i16;
            w.write_sample(v).expect("sample");
        }
        w.finalize().expect("finalize");
    }
    cursor.into_inner()
}

const BLOCK_FRAMES: usize = 512;
const WARMUP_BLOCKS: usize = 500;
const MEASURE_BLOCKS: usize = 3000;

fn main() {
    let bytes = Arc::new(make_wav_bytes(30, 48_000));
    let mut sources: Vec<Option<Box<dyn AudioSource>>> = Vec::new();
    for _ in 0..8 {
        let src: Box<dyn symphonia::core::io::MediaSource> = Box::new(Cursor::new(bytes.to_vec()));
        let (r, _) = open_source(src).expect("open");
        sources.push(Some(Box::new(r) as Box<dyn AudioSource>));
    }

    let mut mixer = Mixer::new();
    for _ in 0..sources.len() {
        mixer.add_track().expect("≤32 track");
    }
    let mut out = vec![0.0f32; BLOCK_FRAMES * 2];

    // Warm-up: prefetch đầy + làm nóng page cache/branch.
    std::thread::sleep(Duration::from_millis(300));
    for _ in 0..WARMUP_BLOCKS {
        mixer.render(&mut sources, &mut out);
        rewind_finished(&mut sources);
    }

    let mut durations_ns: Vec<u128> = Vec::with_capacity(MEASURE_BLOCKS);
    for _ in 0..MEASURE_BLOCKS {
        let t0 = Instant::now();
        mixer.render(&mut sources, &mut out);
        durations_ns.push(t0.elapsed().as_nanos());
        rewind_finished(&mut sources);
        // Pace nhẹ để không chạy nhanh hơn worker quá mức — bench đo thời
        // gian BLOCK, không phải throughput.
        std::thread::sleep(Duration::from_micros(150));
    }

    durations_ns.sort_unstable();
    let mean = durations_ns.iter().sum::<u128>() / durations_ns.len() as u128;
    let p50 = durations_ns[durations_ns.len() / 2];
    let p99 = durations_ns[durations_ns.len() * 99 / 100];
    let max = durations_ns.last().copied().unwrap_or(0);

    println!("blocks={BLOCK_FRAMES} measured={MEASURE_BLOCKS} p50={p50}ns p99={p99}ns max={max}ns");
    // Dòng bencher để scripts/collect-bench.mjs parse (job `bench` trên CI
    // pipe stdout qua collector — BENCH_MAP.audio_callback_budget_ratio).
    // mean của MỘT block 512 frame = thời gian một audio callback.
    println!(
        "test bench_audio_mixer_callback ... bench: {:>11} ns/iter (+/- 0)",
        mean
    );

    const BUDGET_NS: u128 = 1_000_000; // 1ms
    assert!(
        mean < BUDGET_NS,
        "mean {mean}ns vượt ngân sách {BUDGET_NS}ns"
    );
    assert!(p99 < BUDGET_NS, "p99 {p99}ns vượt ngân sách {BUDGET_NS}ns");
}

/// Track nào phát hết thì seek về 0 — ngoài vùng đo của block.
fn rewind_finished(sources: &mut [Option<Box<dyn AudioSource>>]) {
    for s in sources.iter_mut().flatten() {
        if s.is_finished() {
            s.seek_frames(0);
        }
    }
}
