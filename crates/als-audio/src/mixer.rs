//! Mixer: cộng dồn các track vào bus master. HÀM THUẦN trên slice — không cấp
//! phát, không I/O, không panic. Đây là phần được golden test so byte.
//!
//! Quy ước:
//! - gain LINEAR (control thread đã qua db_to_linear).
//! - pan constant-power: gL = cos((pan+1)·π/4), gR = sin((pan+1)·π/4),
//!   TÍNH SẮN lúc set_pan — không gọi lượng giác trong vòng lặp render.
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
    ///
    /// QUAN TRỌNG — track im lặng VẪN phải được kéo: source ở v1 là buffer
    /// consolidated timeline-absolute, con trỏ của nó chỉ nhúc nhích khi
    /// `next_frame()` được gọi. Nếu `continue` thậng khi mute thì sau 10 giây
    /// mute, con trỏ trễ 10 giây so với transport → bỏ mute là nghe lại đoạn
    /// cũ, lệch nhịp vĩnh viễn. Nên ở đây kéo và bỏ đi (vẫn không cấp phát).
    pub fn render(&mut self, sources: &mut [Option<Box<dyn AudioSource>>], out: &mut [f32]) {
        let frames = out.len() / 2;
        for s in out.iter_mut() {
            *s = 0.0;
        }
        let solo_active = self.any_solo();

        for i in 0..self.track_count {
            let state = self.tracks[i];
            let silent = state.mute || (solo_active && !state.solo);
            let Some(src) = sources.get_mut(i).and_then(Option::as_mut) else {
                continue;
            };
            if silent {
                // Kéo để con trỏ theo kịp transport, không cộng vào out.
                for _ in 0..frames {
                    let _ = src.next_frame();
                }
                continue;
            }
            for f in 0..frames {
                let (l, r) = src.next_frame();
                // Một get_mut cho cả cặp kênh — hai lời gọi trong cùng expression
                // là borrow đôi (E0499). Range OOB → None → bỏ qua, không panic.
                if let Some(pair) = out.get_mut(f * 2..f * 2 + 2) {
                    pair[0] += l * state.gain_l;
                    pair[1] += r * state.gain_r;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::BufferSource;

    /// Bước ramp nhỏ CÓ CHỦ ĐÍCH: master clamp kẹp output về [-1, 1]. Nếu mẫu
    /// lớn (1.0, 2.0, 3.0...) thì sau khi nhân gain_l ≈ 0.707 vẫn vượt 1.0 và bị
    /// kẹp → test mất khả năng phân biệt con trỏ đang ở frame nào. Đừng nâng.
    const RAMP_STEP: f32 = 0.01;

    /// Source đếm tăng dần để biết con trỏ đã đi tới đâu chỉ bằng giá trị mẫu:
    /// frame thứ n mang giá trị n * RAMP_STEP trên cả hai kênh.
    fn ramp(frames: usize) -> Box<dyn AudioSource> {
        let mut data = Vec::with_capacity(frames * 2);
        for f in 0..frames {
            let v = (f + 1) as f32 * RAMP_STEP;
            data.push(v);
            data.push(v);
        }
        Box::new(BufferSource::from_interleaved(data))
    }

    /// Regression: track bị mute phải tiêu thụ đúng số frame như track phát,
    /// nếu không bỏ mute sẽ nghe lại đoạn cũ và lệch nhịp mãi mãi.
    #[test]
    fn muted_track_still_advances_source() {
        let mut mixer = Mixer::new();
        let _ = mixer.add_track();
        mixer.tracks[0].mute = true;
        let mut sources: Vec<Option<Box<dyn AudioSource>>> = vec![Some(ramp(8))];

        let mut out = [0.0f32; 4]; // 2 frame
        mixer.render(&mut sources, &mut out);
        assert_eq!(out, [0.0; 4], "mute phải im lặng");

        // Bỏ mute: phải nghe frame THỨ 3, không phải frame 1.
        mixer.tracks[0].mute = false;
        let mut out2 = [0.0f32; 2]; // 1 frame
        mixer.render(&mut sources, &mut out2);
        let expected = 3.0 * RAMP_STEP * mixer.tracks[0].gain_l;
        assert!(
            (out2[0] - expected).abs() < 1e-6,
            "con trỏ source phải đã đi qua 2 frame bị mute: mong {expected}, nhận {}",
            out2[0]
        );
    }

    /// Solo track khác cũng làm track này im — cùng phải giữ nhịp.
    #[test]
    fn solo_elsewhere_still_advances_source() {
        let mut mixer = Mixer::new();
        let _ = mixer.add_track();
        let _ = mixer.add_track();
        mixer.tracks[1].solo = true;
        let mut sources: Vec<Option<Box<dyn AudioSource>>> = vec![Some(ramp(8)), None];

        let mut out = [0.0f32; 4]; // 2 frame
        mixer.render(&mut sources, &mut out);
        assert_eq!(out, [0.0; 4]);

        mixer.tracks[1].solo = false;
        let mut out2 = [0.0f32; 2];
        mixer.render(&mut sources, &mut out2);
        let expected = 3.0 * RAMP_STEP * mixer.tracks[0].gain_l;
        assert!((out2[0] - expected).abs() < 1e-6);
    }

    #[test]
    fn master_clamps_to_unit_range() {
        let mut mixer = Mixer::new();
        let _ = mixer.add_track();
        let mut sources: Vec<Option<Box<dyn AudioSource>>> =
            vec![Some(Box::new(BufferSource::from_interleaved(vec![
                10.0, -10.0,
            ])))];
        let mut out = [0.0f32; 2];
        mixer.render(&mut sources, &mut out);
        // Giá trị sau clamp là xác định, nên so chính xác thay vì <= / >=.
        assert_eq!(out[0], 1.0, "đỉnh dương phải bị kẹp đúng 1.0");
        assert_eq!(out[1], -1.0, "đỉnh âm phải bị kẹp đúng -1.0");
    }
}
