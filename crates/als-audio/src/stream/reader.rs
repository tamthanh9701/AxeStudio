//! `StreamingReader` — impl [`AudioSource`] kéo dữ liệu từ ring buffer do
//! prefetch thread đổ vào. Đường callback (next_frame/seek_frames/
//! is_finished/len_frames) KHÔNG cấp phát, KHÔNG khoá, KHÔNG I/O.
//!
//! Worker chuyển mọi nguồn về **stereo interleaved** (mono nhân đôi kênh) để
//! đường RT không phải biết chuyện format.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rtrb::{Consumer, Producer, RingBuffer};

use crate::source::AudioSource;
use crate::stream::decoder::{
    open_path as dec_open_path, open_source as dec_open_source, Opened, PacketDecoder,
};
use crate::stream::{StreamingError, PREFETCH_SECONDS};
use crate::xrun::XrunCounter;

/// Frames mỗi chunk: ~85ms @48kHz stereo = 16KB f32. Đủ nhỏ để phần stale
/// sau seek ngắn, đủ lớn để overhead pop không đáng kể.
const CHUNK_FRAMES: usize = 4096;
/// Worker ngủ khi queue đầy / đã EOF — chờ seek mới hoặc reader tiêu bớt.
const POLL_BUSY: Duration = Duration::from_millis(2);
const POLL_IDLE: Duration = Duration::from_millis(10);

/// Trạng thái chia sẻ worker ↔ reader. Chỉ atomics — không khoá (AGENTS §3).
struct Shared {
    /// Tăng mỗi lần seek. Chunk gắn epoch lúc DECODE — nhỏ hơn epoch hiện
    /// tại là stale, reader vứt im lặng.
    epoch: AtomicU64,
    /// Frame đích của lần seek gần nhất (ghi TRƯỚC khi bump epoch).
    seek_target: AtomicU64,
    /// Reader drop → worker thoát trong ≤ POLL_IDLE.
    stop: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            epoch: AtomicU64::new(0),
            seek_target: AtomicU64::new(0),
            stop: AtomicBool::new(false),
        }
    }
}

/// Một khối audio stereo-interleaved đã decode xong.
struct Chunk {
    epoch: u64,
    data: Box<[f32]>,
}

/// Thông tin phía control thread: độ dài file + bộ đếm underrun (clone tự do).
pub struct StreamingInfo {
    pub total_frames: u64,
    pub underruns: XrunCounter,
}

pub struct StreamingReader {
    rx: Consumer<Chunk>,
    shared: Arc<Shared>,
    underruns: XrunCounter,
    total_frames: u64,

    // --- trạng thái RT (chỉ audio callback chạm) ---
    cur: Option<Box<[f32]>>,
    /// Vị trí SAMPLE trong `cur` (luôn bội của 2 — stereo).
    pos_in_cur: usize,
    /// Số frame đã phục vụ tính từ frame 0.
    served: u64,
    /// Đang trong một đợt đói (đếm underrun theo episode, không theo frame).
    starving: bool,
    /// Sức chứa ring (rtrb 0.3 không giữ metadata trên Consumer) — giới hạn
    /// bounded cho refill/drain mỗi callback.
    ring_cap: usize,
}

/// Mở file từ đĩa. Header đọc đồng bộ ở đây (metadata, nhanh) để có
/// `total_frames` ngay; toàn bộ decode chạy ở prefetch thread.
///
/// PHẢI gọi ở control thread — không bao giờ trong audio callback.
pub fn open_path(path: &Path) -> Result<(StreamingReader, StreamingInfo), StreamingError> {
    let (dec, opened) = dec_open_path(path).map_err(StreamingError::Decode)?;
    Ok(finish_open(dec, opened))
}

/// Giống [`open_path`] nhưng nhận nguồn tuỳ ý (`Cursor` cho test in-RAM).
pub fn open_source(
    source: Box<dyn symphonia::core::io::MediaSource>,
) -> Result<(StreamingReader, StreamingInfo), StreamingError> {
    let (dec, opened) = dec_open_source(source).map_err(StreamingError::Decode)?;
    Ok(finish_open(dec, opened))
}

fn finish_open(dec: PacketDecoder, opened: Opened) -> (StreamingReader, StreamingInfo) {
    let total_frames = if opened.total_frames > 0 {
        opened.total_frames
    } else {
        dec.total_frames()
    };

    let frames_wanted = (PREFETCH_SECONDS * opened.sample_rate as f32).ceil() as usize;
    let capacity = frames_wanted / CHUNK_FRAMES + 2;
    let (prod, cons) = RingBuffer::new(capacity);
    let shared = Arc::new(Shared::new());
    let underruns = XrunCounter::new();

    // Worker sở hữu decoder + producer. Detach chủ đích: Drop của reader CHỈ
    // set cờ stop, KHÔNG join — drop có thể xảy ra trên RT thread.
    let worker_shared = Arc::clone(&shared);
    thread::Builder::new()
        .name("als-audio-prefetch".into())
        .spawn(move || run_worker(worker_shared, prod, dec))
        .expect("spawn prefetch thread");

    let info = StreamingInfo {
        total_frames,
        underruns: underruns.clone(),
    };
    (
        StreamingReader {
            rx: cons,
            shared,
            underruns,
            total_frames,
            cur: None,
            pos_in_cur: 0,
            served: 0,
            starving: false,
            ring_cap: capacity,
        },
        info,
    )
}

/// Vòng lặp prefetch. Decode nhanh hơn phát thật nhiều lần nên worker chủ
/// yếu sống ở nhánh "queue đầy, ngủ POLL_BUSY". Mỗi vòng: thấy epoch đổi →
/// seek tới target; rồi cố đổ đầy queue rồi ngủ một nhịp.
///
/// `thread::sleep` bị cấm toàn workspace vì audio callback — nhưng hàm này
/// chạy trên WORKER thread thường, không phải callback: sleep chính là cơ
/// chế nhường CPU chủ đích. Crate này không có (và không được có) tokio
/// runtime để dùng `tokio::time::sleep`.
#[allow(clippy::disallowed_methods)]
fn run_worker(shared: Arc<Shared>, mut prod: Producer<Chunk>, mut dec: PacketDecoder) {
    let mut local_epoch = shared.epoch.load(Ordering::Acquire);
    loop {
        if shared.stop.load(Ordering::Relaxed) {
            return;
        }

        let ep_now = shared.epoch.load(Ordering::Acquire);
        if ep_now != local_epoch {
            local_epoch = ep_now;
            let target = shared.seek_target.load(Ordering::Acquire);
            // Lỗi seek tại vị trí hỏng: bỏ yên — reader silence + đếm
            // underrun thay vì panic. Không log: crate không thêm dep tracing.
            let _ = dec.seek_to(target);
        }

        // Đổ đầy queue; dừng khi đầy (push Err) hoặc hết/lỗi dữ liệu.
        while prod.slots() > 0 {
            match dec.next_samples(CHUNK_FRAMES * dec.channels()) {
                Ok(Some(samples)) => {
                    let data = to_stereo(samples, dec.channels());
                    if prod
                        .push(Chunk {
                            epoch: local_epoch,
                            data,
                        })
                        .is_err()
                    {
                        break; // đầy — nhường reader tiêu
                    }
                    // Epoch đổi giữa chừng: chunk đang cầm là stale (đã gắn
                    // epoch cũ nên reader tự vứt); thoát để vòng sau seek.
                    if shared.epoch.load(Ordering::Acquire) != local_epoch {
                        break;
                    }
                }
                Ok(None) | Err(_) => {
                    // EOF / decode lỗi: ngủ dài hơn, chỉ đánh thức khi seek.
                    thread::sleep(POLL_IDLE);
                    break;
                }
            }
        }

        thread::sleep(POLL_BUSY);
    }
}

/// Mono → nhân đôi kênh thành stereo; stereo → giữ nguyên. >2 kênh bị chặn
/// ở tầng decoder nên chỉ hai nhánh này tồn tại.
fn to_stereo(samples: Vec<f32>, channels: usize) -> Box<[f32]> {
    if channels == 1 {
        samples
            .into_iter()
            .flat_map(|s| [s, s])
            .collect::<Vec<f32>>()
            .into_boxed_slice()
    } else {
        samples.into_boxed_slice()
    }
}

impl StreamingReader {
    /// Frame kế tiếp; hết chunk → pop queue (vứt chunk stale theo epoch);
    /// đói hoàn toàn → silence + tăng underrun. Không panic, không cấp phát.
    #[inline]
    fn next_sample_pair(&mut self) -> (f32, f32) {
        if self.cur.is_none() && !self.refill() {
            if !self.starving {
                self.starving = true;
                self.underruns.bump();
            }
            return (0.0, 0.0);
        }
        self.starving = false;
        let data = self.cur.as_ref().expect("refill bảo đảm Some");
        // get thay indexing — dữ liệu lệch độ dài là bug phía worker, xử lý
        // bằng silence chứ không panic (AGENTS §3).
        let pair = match (data.get(self.pos_in_cur), data.get(self.pos_in_cur + 1)) {
            (Some(&l), Some(&r)) => (l, r),
            _ => (0.0, 0.0),
        };
        self.pos_in_cur += 2;
        self.served += 1;
        if self.pos_in_cur >= data.len() {
            // Nhả chunk ngay để bộ nhớ quay về worker sớm.
            self.cur = None;
            self.pos_in_cur = 0;
        }
        pair
    }

    /// Pop tới khi gặp chunk cùng epoch còn dữ liệu. Số lần pop mỗi call bị
    /// chặn bởi sức chứa ring — công việc mỗi callback luôn bounded.
    fn refill(&mut self) -> bool {
        let epoch = self.shared.epoch.load(Ordering::Acquire);
        for _ in 0..self.ring_cap {
            match self.rx.pop() {
                Ok(c) => {
                    if c.epoch == epoch && !c.data.is_empty() {
                        self.cur = Some(c.data);
                        self.pos_in_cur = 0;
                        return true;
                    }
                    // stale hoặc rỗng → bỏ, thử tiếp
                }
                Err(_) => return false, // queue rỗng
            }
        }
        false
    }

    pub fn has_buffered_data(&self) -> bool {
        // Consumer::slots() = số chunk ĐỌC ĐƯỢC trong ring.
        self.rx.slots() > 0
    }

    /// Số chunk đang nằm trong ring (control-side diagnostics/priming).
    pub fn buffered_chunks(&self) -> usize {
        // Consumer::slots() CHÍNH LÀ số chunk đọc được — không cần trừ.
        self.rx.slots()
    }
}

impl AudioSource for StreamingReader {
    #[inline]
    fn next_frame(&mut self) -> (f32, f32) {
        self.next_sample_pair()
    }

    fn is_finished(&self) -> bool {
        self.served >= self.total_frames
    }

    /// Seek không chặn: ghi target rồi tăng epoch. Worker thấy epoch mới sẽ
    /// huỷ phần decode dở và nạp lại từ vị trí mới. Callback chỉ làm việc
    /// O(capacity): reset con trỏ + dọn queue.
    fn seek_frames(&mut self, frames: u64) {
        let target = frames.min(self.total_frames);
        self.shared.seek_target.store(target, Ordering::Release);
        self.shared.epoch.fetch_add(1, Ordering::AcqRel);

        // Reset phía RT: chunk đang cầm giờ là stale.
        self.cur = None;
        self.pos_in_cur = 0;
        self.served = target;

        // Dọn queue: mọi chunk còn nằm đều sinh trước mốc epoch mới (worker
        // chưa kịp thấy epoch) — vứt hết để worker decode lại từ target mà
        // không kẹt queue đầy. Bounded bởi capacity, không blocking.
        while self.rx.pop().is_ok() {}
    }

    fn len_frames(&self) -> u64 {
        self.total_frames
    }
}

impl Drop for StreamingReader {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}
