//! Lệnh từ control thread → audio thread. MỌI lệnh đi qua MỘT SPSC queue
//! duy nhất (rtrb). Không dùng Mutex để bảo vệ trạng thái transport/mixer.
//!
//! Lưu ý thiết kế: AddTrack KHÔNG nằm ở đây. Track/source được đăng ký lúc
//! build engine (EngineBuilder) để không có con trỏ heap nào bị drop trên RT
//! thread. Nạp source lúc runtime (streaming reader) là ticket ALS-B02, sẽ
//! dùng cơ chế swap double-buffer riêng.

/// Lệnh điều khiển transport + mixer. Tất cả field đều là kiểu Copy —
/// không heap, không drop nặng phía audio thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Play,
    Pause,
    Stop,
    /// Seek tới vị trí tuyệt đối (frame).
    Seek(u64),
    SetLoop {
        start_frames: u64,
        end_frames: u64,
        enabled: bool,
    },
    /// gain LINEAR (đã qua db_to_linear ở control thread).
    SetTrackGain { track: u8, gain: f32 },
    /// -1.0 ..= 1.0
    SetTrackPan { track: u8, pan: f32 },
    SetTrackMute { track: u8, mute: bool },
    SetTrackSolo { track: u8, solo: bool },
}
