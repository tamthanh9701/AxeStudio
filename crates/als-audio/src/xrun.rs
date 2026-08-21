//! Bộ đếm xrun/underrun chia sẻ — audio thread GHI, control thread ĐỌC.
//!
//! Đếm HAI loại sự cố (spike S-08, issue #7):
//! 1. cpal báo lỗi device (xrun thật của driver/WASAPI) — error callback.
//! 2. Underrun phía ta: transport đang phát nhưng MỌI source đều cạn
//!    → silence nghe được (định nghĩa ở engine.rs, sau mixer.render).
//!
//! Cùng pattern với [`crate::playhead::Playhead`] / [`crate::meter::Meters`]:
//! `Arc<Atomic*>`, clone tự do, không khoá, không cấp phát khi bump/load.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct XrunCounter {
    count: Arc<AtomicUsize>,
}

impl XrunCounter {
    pub fn new() -> Self {
        Self {
            count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Audio thread gọi khi phát hiện xrun/underrun. `Relaxed` đủ: counter
    /// chỉ phục vụ đo lường, không đồng bộ hoá dữ liệu nào khác.
    #[inline]
    pub(crate) fn bump(&self) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Control thread đọc định kỳ (Diagnostics, spike S-08 in mỗi phút).
    #[inline]
    pub fn load(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }
}

impl Default for XrunCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_clones_share_state() {
        let c = XrunCounter::new();
        let d = c.clone();
        // bump là pub(crate): test trong crate dùng được trực tiếp.
        for _ in 0..3 {
            c.bump();
        }
        assert_eq!(d.load(), 3);
        assert_eq!(c.load(), 3);
    }
}
