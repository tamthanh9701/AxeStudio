//! Mixer: cộng dồn các track vào bus master. HÀM THUẦN trên slice — không cấp
//! phát, không I/O, không panic. Đây là phần được golden test so byte.
//!
//! Quy ước:
//! - gain LINEAR (control thread đã qua db_to_linear).
//! - pan constant-power: gL = cos((pan+1)·π/4), gR = sin((pan+1)·π/4),
//!   TÍNH SẴN lúc set_pan — không gọi lượng giác trong vòng lặp render.
//! - Master: hard clamp vào [-1, 1]. Limiter thật ở S2; clamp ở đây để golden
//!   test byte-exact.

use crate::source::AudioSource;
use std::f32::consts::FRAC_PI_4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackState {
    pub gain: f32,
    pub pan: f32,
    pub gain_l: f32,
    pub gain_r: f32,
    pub mute: bool,
    pub solo: bool,
}

impl Default for TrackState {
    fn default() -> Self {
        let mut s = Self {
            gain: 1.0,
            pan: 0.0,
            gain_l: 0.0,
            gain_r: 0.0,
            mute: false,
            solo: false,
        };
        s.update_pan_gains();
        s
    }
}

impl TrackState {
    /// Pan constant-power. pan ∈ [-1, 1] → góc ∈ [0, π/2].
    #[inline]
    pub fn update_pan_gains(&mut self) {
        let angle = (self.pan.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
        self.gain_l = self.gain * angle.cos();
        self.gain_r = self.gain * angle.sin();
    }

    #[inline]
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain;
        self.update_pan_gains();
    }

    #[inline]
    pub fn set_pan(&mut self, pan: f32) {
        self.pan = pan.clamp(-1.0, 1.0);
        self.update_pan_gains();
    }
}

/// Mixer tối đa N track cố định — mảng tĩnh, không Vec, không cấp phát.
pub const MAX_TRACKS: usize = 32;

pub struct Mixer {
    pub tracks: [TrackState; MAX_TRACKS],
    pub track_count: usize,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Mixer {
    pub fn new() -> Self {
        Self {
            tracks: [TrackState::default(); MAX_TRACKS],
            track_count: 0,
        }
    }

    pub fn add_track(&mut self) -> Option<usize> {
        if self.track_count >= MAX_TRACKS {
            return None;
        }
        let idx = self.track_count;
        self.tracks[idx] = TrackState::default();
        self.track_count += 1;
        Some(idx)
    }

    /// Có track nào solo không — tính một lần mỗi block, không mỗi frame.
    #[inline]
    fn any_solo(&self) -> bool {
        self.tracks[..self.track_count].iter().any(|t| t.solo)
    }

    /// Render `frames` frame stereo vào `out` (độ dài frames*2), kéo từ
    /// `sources`. out được ZERO trước — mixer cộng dồn, không ghi đè.
    ///
    /// RT-safe: không cấp phát, không panic. sources thiếu dữ liệu → silence.
    pub fn render(&mut self, sources: &mut [Option<Box<dyn AudioSource>>], out: &mut [f32]) {
        let frames = out.len() / 2;
        for s in out.iter_mut() {
            *s = 0.0;
        }
        let solo_active = self.any_solo();

        for i in 0..self.track_count {
            let state = self.tracks[i];
            if state.mute || (solo_active && !state.solo) {
                continue;
            }
            let Some(src) = sources.get_mut(i).and_then(Option::as_mut) else {
                continue;
            };
            for f in 0..frames {
                let (l, r) = src.next_frame();
                // get() thay indexing — không bao giờ panic trong RT path.
                if let (Some(ol), Some(or)) = (out.get_mut(f * 2), out.get_mut(f * 2 + 1)) {
                    *ol += l * state.gain_l;
                    *or += r * state.gain_r;
                }
            }
        }

        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }

    pub fn apply(&mut self, cmd: &crate::command::Command) {
        use crate::command::Command::*;
        match *cmd {
            SetTrackGain { track, gain } => {
                if let Some(t) = self.tracks.get_mut(track as usize) {
                    t.set_gain(gain);
                }
            }
            SetTrackPan { track, pan } => {
                if let Some(t) = self.tracks.get_mut(track as usize) {
                    t.set_pan(pan);
                }
            }
            SetTrackMute { track, mute } => {
                if let Some(t) = self.tracks.get_mut(track as usize) {
                    t.mute = mute;
                }
            }
            SetTrackSolo { track, solo } => {
                if let Some(t) = self.tracks.get_mut(track as usize) {
                    t.solo = solo;
                }
            }
            _ => {}
        }
    }
}
