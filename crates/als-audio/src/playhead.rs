//! Playhead chia sẻ: audio thread GHI, UI thread ĐỌC trong requestAnimationFrame.
//! Không bao giờ bắn event IPC mỗi frame (contract ipc.md §KHÔNG qua event).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct Playhead {
    frames: Arc<AtomicU64>,
}

impl Playhead {
    pub fn new() -> Self {
        Self {
            frames: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Audio thread gọi sau mỗi block render.
    #[inline]
    pub fn store_frames(&self, frames: u64) {
        self.frames.store(frames, Ordering::Release);
    }

    /// UI thread gọi trong rAF.
    #[inline]
    pub fn load_frames(&self) -> u64 {
        self.frames.load(Ordering::Acquire)
    }

    #[inline]
    pub fn load_ms(&self, sample_rate: u32) -> f64 {
        self.load_frames() as f64 * 1000.0 / f64::from(sample_rate)
    }
}

impl Default for Playhead {
    fn default() -> Self {
        Self::new()
    }
}
