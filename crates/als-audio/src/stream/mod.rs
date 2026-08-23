//! Streaming reader — phát file WAV/FLAC từ đĩa không nạp hết vào RAM
//! (issue #9 / ALS-B02).
//!
//! ## Kiến trúc
//!
//! ```text
//!  control thread                 prefetch thread            audio callback
//!  ──────────────                 ───────────────            ──────────────
//!  StreamingReader::open ───► PacketDecoder ── Chunk ──► rtrb ──► AudioSource
//!  (mở header, đọc tổng số       (decode + seek,          (pop, KHÔNG cấp phát,
//!   frame) rồi move decoder      được phép malloc)         silence khi đói)
//!   vào worker
//! ```
//!
//! - Giao tiếp duy nhất giữa worker và callback: **SPSC ring** (`rtrb`) các
//!   `Chunk` đã decode sẵn ~[`PREFETCH_SECONDS`] giây.
//! - Seek không chặn callback: callback tăng **epoch** (atomic) + ghi target;
//!   chunk mang epoch lúc decode — chunk cũ bị vứt im lặng khi pop.
//! - Underrun → trả silence + [`crate::XrunCounter`] tăng, không panic
//!   (AGENTS.md §3).
//!
//! ## CẤM trong đường callback (test `no_alloc` kèm theo chứng minh)
//! malloc · Vec::push · Mutex/RwLock · println!/log · I/O · panic.

mod decoder;
mod reader;

pub use decoder::DecodeError;
pub use reader::{open_path, open_source, StreamingInfo, StreamingReader};

/// Số giây audio được decode TRƯỚC (mục tiêu của ring buffer).
pub const PREFETCH_SECONDS: f32 = 2.0;

/// Lỗi mở/đọc file streaming.
#[derive(Debug, thiserror::Error)]
pub enum StreamingError {
    #[error(transparent)]
    Decode(#[from] DecodeError),
}
