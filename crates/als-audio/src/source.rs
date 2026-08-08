//! Nguồn audio cho mixer. Trait được thiết kế để mixer gọi được trong RT
//! callback: next_frame không cấp phát, không I/O, không panic.

/// Một nguồn stereo interleaved. Mixer kéo từng frame.
pub trait AudioSource: Send {
    /// Frame kế tiếp (L, R). Hết dữ liệu → (0.0, 0.0), KHÔNG panic.
    fn next_frame(&mut self) -> (f32, f32);
    fn is_finished(&self) -> bool;
    /// Seek tương đối theo frame từ đầu source. Clamp nếu vượt cuối.
    fn seek_frames(&mut self, frames: u64);
    fn len_frames(&self) -> u64;
}

/// Nguồn in-memory: toàn bộ audio nằm trong RAM (đã decode từ trước).
/// Streaming từ đĩa (ALS-B02) sẽ là một impl khác của trait này.
pub struct BufferSource {
    /// Interleaved stereo: [L0, R0, L1, R1, ...]
    data: Vec<f32>,
    pos: usize,
}

impl BufferSource {
    pub fn from_interleaved(data: Vec<f32>) -> Self {
        // Làm tròn xuống số frame chẵn kênh — dữ liệu lẻ là bug phía caller,
        // ở đây chọn im lặng cắt bớt thay vì panic trong RT path.
        let frames = data.len() / 2;
        Self {
            data,
            pos: 0,
        }
        .with_len(frames)
    }

    fn with_len(mut self, frames: usize) -> Self {
        self.data.truncate(frames * 2);
        self
    }
}

impl AudioSource for BufferSource {
    #[inline]
    fn next_frame(&mut self) -> (f32, f32) {
        // Dùng get() thay indexing: hết dữ liệu trả silence, không panic.
        match (self.data.get(self.pos), self.data.get(self.pos + 1)) {
            (Some(&l), Some(&r)) => {
                self.pos += 2;
                (l, r)
            }
            _ => (0.0, 0.0),
        }
    }

    #[inline]
    fn is_finished(&self) -> bool {
        self.pos + 1 >= self.data.len()
    }

    fn seek_frames(&mut self, frames: u64) {
        let max = (self.data.len() / 2) as u64;
        self.pos = (frames.min(max) * 2) as usize;
    }

    fn len_frames(&self) -> u64 {
        (self.data.len() / 2) as u64
    }
}
