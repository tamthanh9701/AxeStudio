//! Transport: play/pause/seek/loop. Chỉ sống trong audio callback — mọi thay
//! đổi từ ngoài đi qua Command. Sample-accurate: seek tính bằng frame, không ms.

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transport {
    pub playing: bool,
    /// Vị trí hiện tại theo frame, trên timeline project.
    pub position_frames: u64,
    pub loop_start_frames: u64,
    pub loop_end_frames: u64,
    pub looping: bool,
}

impl Transport {
    pub fn apply(&mut self, cmd: &crate::command::Command) {
        use crate::command::Command::*;
        match *cmd {
            Play => self.playing = true,
            Pause => self.playing = false,
            Stop => {
                self.playing = false;
                self.position_frames = 0;
            }
            Seek(frames) => self.position_frames = frames,
            SetLoop {
                start_frames,
                end_frames,
                enabled,
            } => {
                self.loop_start_frames = start_frames;
                self.loop_end_frames = end_frames.max(start_frames);
                self.looping = enabled;
            }
            _ => {}
        }
    }

    /// Gọi sau khi render một block. Trả về vị trí mới, xử lý wrap loop.
    #[inline]
    pub fn advance(&mut self, frames: u64) {
        if !self.playing {
            return;
        }
        self.position_frames += frames;
        if self.looping
            && self.loop_end_frames > self.loop_start_frames
            && self.position_frames >= self.loop_end_frames
        {
            // Wrap về đầu loop, giữ phần dư để không lệch pha.
            let span = self.loop_end_frames - self.loop_start_frames;
            let over = self.position_frames - self.loop_end_frames;
            self.position_frames = self.loop_start_frames + (over % span);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;

    #[test]
    fn loop_wraps_sample_accurately() {
        let mut t = Transport::default();
        t.apply(&Command::Play);
        t.apply(&Command::SetLoop {
            start_frames: 1000,
            end_frames: 2000,
            enabled: true,
        });
        t.position_frames = 1990;
        t.advance(20); // vượt 10 frame qua mốc 2000
        assert_eq!(
            t.position_frames, 1010,
            "phần dư phải được giữ lại sau wrap"
        );
    }

    #[test]
    fn stop_resets_position() {
        let mut t = Transport {
            playing: true,
            position_frames: 12345,
            ..Default::default()
        };
        t.apply(&Command::Stop);
        assert!(!t.playing);
        assert_eq!(t.position_frames, 0);
    }
}
