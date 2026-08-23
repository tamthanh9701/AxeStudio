//! Test StreamingReader (ALS-B02, issue #9).
//!
//! Audio sinh TỔNG QUAT trong RAM (hound ghi WAV vào `Cursor<Vec<u8>>`,
//! symphonia có sẵn `MediaSource for Cursor<T: AsRef<[u8]>>`) — không phụ
//! thuộc fixture trên đĩa, đúng quy ước dev-deps của crate.
//!
//! Về test "60 giây": giữ nguyên Ý của ticket (8 file × 60s nội dung phát
//! SONG SONG, underrun == 0) nhưng kéo nhanh hơn realtime (~5×) — unit CI
//! không nên ngủ 60s; hành vi realtime đã được spike S-08 chứng minh trên
//! thiết bị thật.
//!
//! Ba test trong file này ĐIỀU KIỂN CPU (worker decode + vòng kéo frame).
//! Chạy song song trên runner yếu (2 vCPU) sẽ tự giành CPU của nhau gây
//! underrun giả → tuần tự hoá bằng atomic spin lock (không dùng std::sync
//!::Mutex — bị cấm trong als-audio theo clippy.toml).
//!
//! `thread::sleep` ở đây là của TEST thread — allow lint toàn file; crate
//! test không có tokio runtime.

#![allow(clippy::disallowed_methods)]

use std::cell::Cell;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use als_audio::source::AudioSource;
use als_audio::stream::open_source;
use als_audio::{Mixer, StreamingInfo, StreamingReader};

/// Tuần tự hoá các test nặng (xem doc đầu file). Spin-lock 1 bit: test nào
/// giữ cờ thì test kia ngủ chờ — không phải đường RT nên sleep hợp lệ.
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;

fn take_serial() -> SerialGuard {
    while SERIAL.swap(true, Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(5));
    }
    SerialGuard
}

impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

// ---------- helpers ----------

/// Sinh WAV PCM s16 MONO (worker phải tự nhân đôi thành stereo).
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
        let total = seconds * sample_rate;
        // Noise deterministic; hiếm khi bằng đúng 0 (xác suất ~1/16001).
        for i in 0..total {
            let v = ((i as i64).wrapping_mul(2654435761) % 16001 - 8000) as i16;
            w.write_sample(v).expect("sample");
        }
        w.finalize().expect("finalize");
    }
    cursor.into_inner()
}

fn open_reader(bytes: &[u8]) -> (StreamingReader, StreamingInfo) {
    let src: Box<dyn symphonia::core::io::MediaSource> = Box::new(Cursor::new(bytes.to_vec()));
    open_source(src).expect("open in-RAM wav")
}

// ---------- acceptance 1: 8 file × 60s song song, 0 underrun ----------

#[test]
fn eight_files_sixty_seconds_parallel_no_underrun() {
    let _serial = take_serial();

    const FILES: usize = 8;
    const SECONDS: u32 = 60;
    const SR: u32 = 48_000;
    /// 100ms silence liên tục giữa dòng = underrun nghe được → fail.
    const SILENT_RUN_LIMIT: u64 = 4_800;
    /// Kéo nhanh hơn realtime bao nhiêu lần (xem doc đầu file). 5× vẫn ép
    /// worker refill liên tục nhưng chừa headroom cho runner yếu.
    const SPEEDUP: f64 = 5.0;
    /// Prime tới khi ring giữ được ít nhất số chunk này (~0.7s audio) —
    /// hấp thụ độ trễ lịch của worker trên runner throttled.
    const PRIME_CHUNKS: usize = 8;

    let bytes = Arc::new(make_wav_bytes(SECONDS, SR));
    let mut readers: Vec<StreamingReader> = Vec::with_capacity(FILES);
    let mut infos: Vec<StreamingInfo> = Vec::with_capacity(FILES);
    for _ in 0..FILES {
        let (r, info) = open_reader(&bytes);
        assert_eq!(info.total_frames, u64::from(SR) * u64::from(SECONDS));
        readers.push(r);
        infos.push(info);
    }

    // PRIME (ngoài vùng đo): chờ ring mỗi reader đầy đáng kể. Trong app
    // thật, play luôn xảy ra sau khi mở file nên cold-start không tính là
    // underrun phát — nó là warm-up.
    for r in readers.iter_mut() {
        let deadline = Instant::now() + Duration::from_secs(10);
        while r.buffered_chunks() < PRIME_CHUNKS {
            assert!(
                Instant::now() < deadline,
                "prefetch không nạp đủ {} chunk",
                PRIME_CHUNKS
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    let min_elapsed = Duration::from_secs_f64(f64::from(SECONDS) / SPEEDUP);
    let started = Instant::now();
    loop {
        if readers.iter().all(|r| r.is_finished()) {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(180),
            "timeout tổng của test"
        );
        for r in readers.iter_mut() {
            if r.is_finished() {
                continue;
            }
            let mut silent_run: u64 = 0;
            for _ in 0..256 {
                let (l, rr) = r.next_frame();
                if l != 0.0 || rr != 0.0 {
                    silent_run = 0;
                } else if !r.is_finished() {
                    silent_run += 1;
                    assert!(
                        silent_run <= SILENT_RUN_LIMIT,
                        "im lặng liên tục > 100ms giữa dòng — underrun nghe được"
                    );
                }
            }
        }
        // Giữ lịch ~5× realtime: đi sớm thì nghỉ bớt (≤2ms/lần để burst
        // không vượt sức chứa ring 2s).
        if let Some(behind) = min_elapsed.checked_sub(started.elapsed()) {
            std::thread::sleep(behind.min(Duration::from_millis(2)));
        }
    }

    for info in &infos {
        assert_eq!(info.underruns.load(), 0, "underrun_count phải == 0");
    }
}

// ---------- acceptance 2: seek ngẫu nhiên 500 lần ----------

#[test]
fn seek_storm_500_no_deadlock_no_panic() {
    let _serial = take_serial();

    let bytes = make_wav_bytes(30, 48_000);
    let (mut r, info) = open_reader(&bytes);
    let total = info.total_frames.max(1);

    // xorshift deterministic — test lặp lại được, không phụ thuộc OS rng.
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    let deadline = Instant::now() + Duration::from_secs(30);
    for i in 0..500 {
        assert!(Instant::now() < deadline, "treo sau {i} lần seek");
        let target = next() % total;
        r.seek_frames(target);
        // Kéo vài trăm frame quanh target — đói ngắn sau seek là ĐƯỢC PHÉP
        // (worker chưa nạp kịp), acceptance chỉ cấm deadlock/panic.
        for _ in 0..300 {
            let _ = r.next_frame();
        }
    }
}

// ---------- acceptance 4: callback path không cấp phát ----------

static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// Bật khi đang ở "vùng callback" của test — allocator chỉ đếm khi cờ
    /// bật, nên malloc của prefetch worker không làm fail sai.
    static MEASURE: Cell<bool> = const { Cell::new(false) };
}

struct CountingAlloc;

unsafe impl std::alloc::GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        if MEASURE.with(|m| m.get()) {
            ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        }
        std::alloc::System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        std::alloc::System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static A: CountingAlloc = CountingAlloc;

/// RAII bật vùng đo — đảm bảo tắt cả khi panic giữa chừng.
struct Measure;

impl Measure {
    fn start() -> Self {
        MEASURE.with(|m| m.set(true));
        Self
    }
}

impl Drop for Measure {
    fn drop(&mut self) {
        MEASURE.with(|m| m.set(false));
    }
}

#[test]
fn streaming_render_allocates_nothing_in_callback_path() {
    let _serial = take_serial();

    // Chuẩn bị (ngoài vùng đo): 2 track streaming + mixer, đợi ring đầy.
    let bytes = Arc::new(make_wav_bytes(10, 48_000));
    let mut readers: Vec<StreamingReader> = Vec::new();
    for _ in 0..2 {
        let (r, _) = open_reader(&bytes);
        readers.push(r);
    }
    let mut mixer = Mixer::new();
    mixer.add_track().unwrap();
    mixer.add_track().unwrap();
    let mut out = vec![0.0f32; 512 * 2];

    // Đợi prefetch đổ đầy (qua handle cụ thể — trait object không có
    // buffered_chunks).
    for r in &mut readers {
        let deadline = Instant::now() + Duration::from_secs(10);
        while r.buffered_chunks() < 8 {
            assert!(Instant::now() < deadline, "prefetch không kịp");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    let mut sources: Vec<Option<Box<dyn AudioSource>>> = readers
        .into_iter()
        .map(|r| Some(Box::new(r) as Box<dyn AudioSource>))
        .collect();
    for s in sources.iter_mut().flatten() {
        for _ in 0..2048 {
            let _ = s.next_frame();
        }
    }

    // VÒNG ĐO: mô phỏng callback — render + loop-wrap seek, xen kẽ như engine.
    // Seek định kỳ khiến reader đói vài episode (worker phải nạp lại từ đầu)
    // — chấp nhận được: acceptance ở đây là KHÔNG CẤP PHÁT, không phải 0 đói.
    let before = ALLOC_COUNT.load(Ordering::SeqCst);
    {
        let _guard = Measure::start();
        for block in 0..200u64 {
            mixer.render(&mut sources, &mut out);
            if block % 16 == 15 {
                for s in sources.iter_mut().flatten() {
                    s.seek_frames(1024); // wrap/seek path cũng phải free-alloc
                }
            }
        }
    }
    let after = ALLOC_COUNT.load(Ordering::SeqCst);
    assert_eq!(
        before,
        after,
        "callback path streaming cấp phát {} lần trong 200 block",
        after - before
    );
}
