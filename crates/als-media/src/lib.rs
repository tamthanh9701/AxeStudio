//! als-media — decode, resample, loudness. Chạy NGOÀI realtime thread.
//!
//! Ranh giới: decode hot path (WAV/FLAC/MP3) dùng symphonia thuần Rust;
//! format lạ và encode AAC/Opus/Mp3 export là FFmpeg sidecar ở als-desktop
//! (plan §5). Crate này không chứa audio callback.

#![forbid(unsafe_code)]

pub mod decode;
pub mod loudness;
pub mod resample;

use thiserror::Error;

/// Buffer stereo/multi-channel interleaved f32, chuẩn hoá của toàn hệ thống.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioBuffer {
    /// Interleaved: [L0, R0, L1, R1, ...]
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u32,
}

impl AudioBuffer {
    pub fn frames(&self) -> usize {
        if self.channels == 0 {
            return 0;
        }
        self.samples.len() / self.channels as usize
    }

    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        (self.frames() as u64 * 1000) / u64::from(self.sample_rate)
    }
}

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("decode: {0}")]
    Decode(String),
    #[error("không có track audio nào")]
    NoAudioTrack,
    #[error("resample: {0}")]
    Resample(String),
    #[error("loudness: {0}")]
    Loudness(String),
    #[error("định dạng không hỗ trợ: {0}")]
    Unsupported(String),
}
