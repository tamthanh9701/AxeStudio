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

    #[test]
    fn db_conversion() {
        assert!((db_to_linear(0.0) - 1.0).abs() < 1e-6);
        assert!((db_to_linear(-6.0206) - 0.501_187_2).abs() < 1e-4);
    }
}
