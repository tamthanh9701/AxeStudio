//! als-audio — audio engine realtime. Crate ĐỘC LẬP: không phụ thuộc crate
//! nội bộ nào, kể cả als-core (AGENTS.md §2).
//!
//! Luật realtime (AGENTS.md §3) — trong audio callback:
//! - CẤM cấp phát heap, CẤM Mutex/RwLock, CẤM I/O, CẤM println!/log.
//! - CẤM panic!/unwrap()/expect()/indexing có thể out-of-bounds.
//! - Underrun → trả silence + tăng counter, không bao giờ panic.
//!
//! Kiến trúc: control thread gửi [`Command`] qua SPSC lock-free (`rtrb`);
//! trạng thái ngược về UI (playhead, meter) đi qua `Atomic*`, KHÔNG qua event.

#![forbid(unsafe_code)]

pub mod command;
pub mod engine;
pub mod meter;
pub mod mixer;
pub mod playhead;
pub mod rt_guard;
pub mod source;
pub mod transport;

pub use command::Command;
pub use engine::{AudioConfig, AudioError, Engine, EngineBuilder};
pub use meter::{MeterReading, Meters};
pub use mixer::{Mixer, TrackState};
pub use playhead::Playhead;
pub use source::{AudioSource, BufferSource};

/// Đổi dB → linear. Mixer chỉ làm việc với gain linear để golden test đơn giản;
/// chuyển đổi xảy ra ở biên command (control thread), không trong callback.
#[inline]
pub fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CẢNH BÁO cho người sửa test này: -6 dB và "nửa biên độ" KHÔNG phải cùng
    /// một số. 20·log10(0.5) = -6.020599913, nên:
    ///   db_to_linear(-6.0206) ≈ 0.5
    ///   db_to_linear(-6.0)    ≈ 0.501_187_2  (= 10^(-0.3))
    /// Bản đầu của test này ghép -6.0206 dB với 0.501_187_2 → lệch 1.19e-3,
    /// vượt ngưỡng 1e-4 và fail. Đó là lỗi của test, không phải của hàm.
    #[test]
    fn db_conversion() {
        // 0 dB = unity gain.
        assert!((db_to_linear(0.0) - 1.0).abs() < 1e-6);
        // Điểm nửa biên độ (fader -6 dB "thật" của tai người).
        assert!((db_to_linear(-6.0206) - 0.5).abs() < 1e-4);
        // -6.0 dB tròn — số khác, chốt luôn để không ai ghép lẫn lần nữa.
        assert!((db_to_linear(-6.0) - 0.501_187_2).abs() < 1e-4);
        // Fader kéo hết xuống = -inf dB → im hoàn toàn.
        // IEEE 754: pow(10, -inf) = +0, nên mute qua đường gain là an toàn.
        assert_eq!(db_to_linear(f32::NEG_INFINITY), 0.0);
        // Đơn điệu tăng — bắt lỗi đảo dấu hoặc chia sai hệ số trong công thức.
        assert!(db_to_linear(-12.0) < db_to_linear(-6.0));
        assert!(db_to_linear(-6.0) < db_to_linear(0.0));
        assert!(db_to_linear(0.0) < db_to_linear(6.0));
    }
}
