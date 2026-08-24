//! Bench `PeakMipmap::compute` trên audio dài 3 phút (ngân sách
//! `docs/perf-budget.md`: peaks_3min_ms ≤ 800).
//!
//! harness=false — tự đo và in dòng bencher để scripts/collect-bench.mjs
//! parse (`test bench_peaks_3min ... bench: N ns/iter`). ns/iter = thời gian
//! compute MỘT lượt trên buffer 3 phút stereo.

#![allow(clippy::disallowed_methods)]

use std::time::Instant;

use als_assets::peaks::PeakMipmap;

const SECONDS: u32 = 180;
const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u32 = 2;
const ITERATIONS: usize = 10;

fn main() {
    // Synth noise deterministic — dữ liệu thật hay giả thì chi phí compute
    // như nhau (quét toàn bộ sample, không early-exit).
    let total = (SECONDS * SAMPLE_RATE * CHANNELS) as usize;
    let samples: Vec<f32> = (0..total)
        .map(|i| ((i as i64).wrapping_mul(2654435761) % 16001) as f32 / 8000.0 - 1.0)
        .collect();

    // Warm một lượt (page cache + branch predictor).
    let _ = PeakMipmap::compute(&samples, CHANNELS, SAMPLE_RATE).expect("peaks compute");

    let mut durations_ns: Vec<u128> = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        let t0 = Instant::now();
        let mm = PeakMipmap::compute(&samples, CHANNELS, SAMPLE_RATE).expect("peaks compute");
        durations_ns.push(t0.elapsed().as_nanos());
        // Đụng vào kết quả để compiler không tối ưu mất phép tính.
        assert!(!mm.encode().is_empty());
    }

    durations_ns.sort_unstable();
    let mean = durations_ns.iter().sum::<u128>() / durations_ns.len() as u128;

    println!(
        "samples={total} iterations={ITERATIONS} mean={mean}ns min={}ns max={}ns",
        durations_ns.first().copied().unwrap_or(0),
        durations_ns.last().copied().unwrap_or(0),
    );
    println!(
        "test bench_peaks_3min ... bench: {:>11} ns/iter (+/- 0)",
        mean
    );
}
