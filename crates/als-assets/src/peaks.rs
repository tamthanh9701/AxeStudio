//! Peaks mipmap — dữ liệu waveform đa mức cho timeline (plan §UI modules).
//!
//! Mỗi mức gom N sample thành một cặp (min, max) f32. Khi zoom, UI chọn mức
//! có spp gần nhất với tỉ lệ pixel → vẽ O(1) bất kể file dài bao nhiêu.
//!
//! Định dạng nhị phân `.alspeak`:
//! ```txt
//! "ALSPK001"     8 bytes magic
//! u32 LE         sample_rate
//! u32 LE         channels
//! u32 LE         số mức
//! per level: u32 spp, u64 số cặp, rồi count × (f32 min, f32 max)
//! u32 LE         crc32 của toàn bộ phần trên
//! ```

use thiserror::Error;

/// Khoảng cách giữa các mức (4 mức, mỗi mức gấp 4 lần): 256, 1024, 4096, 16384.
pub const LEVEL_SPACING: [u32; 4] = [256, 1024, 4096, 16384];

const MAGIC: &[u8; 8] = b"ALSPK001";

#[derive(Debug, Error)]
pub enum PeaksError {
    #[error("peaks: magic sai — file không phải .alspeak")]
    BadMagic,
    #[error("peaks: dữ liệu cắt dở hoặc thiếu byte")]
    Truncated,
    #[error("peaks: crc không khớp — file hỏng")]
    BadCrc,
    #[error("peaks: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PeakLevel {
    /// Samples per peak: 256 / 1024 / 4096 / 16384.
    pub spp: u32,
    /// (min, max) cho mỗi cụm spp sample, trộn cả hai kênh.
    pub pairs: Vec<(f32, f32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PeakMipmap {
    pub sample_rate: u32,
    pub channels: u32,
    pub levels: Vec<PeakLevel>,
}

impl PeakMipmap {
    /// Tính mipmap từ interleaved f32. `samples.len()` phải chia hết cho channels.
    pub fn compute(samples: &[f32], channels: u32, sample_rate: u32) -> Result<Self, PeaksError> {
        if channels == 0 || samples.len() % channels as usize != 0 {
            return Err(PeaksError::Invalid(
                "samples không chia hết cho channels".into(),
            ));
        }
        let frames = samples.len() / channels as usize;
        let levels = LEVEL_SPACING
            .iter()
            .map(|&spp| {
                let mut pairs = Vec::with_capacity(frames.div_ceil(spp as usize));
                let mut i = 0;
                while i < frames {
                    let end = (i + spp as usize).min(frames);
                    let mut lo = f32::INFINITY;
                    let mut hi = f32::NEG_INFINITY;
                    for f in i..end {
                        for c in 0..channels as usize {
                            let s = samples[f * channels as usize + c];
                            if s < lo {
                                lo = s;
                            }
                            if s > hi {
                                hi = s;
                            }
                        }
                    }
                    pairs.push((lo, hi));
                    i = end;
                }
                PeakLevel { spp, pairs }
            })
            .collect();
        Ok(Self {
            sample_rate,
            channels,
            levels,
        })
    }

    /// Chọn mức phù hợp cho tỉ lệ hiển thị: `pixels` cho `duration_ms`.
    pub fn pick(&self, duration_ms: u64, pixels: u32) -> &PeakLevel {
        if pixels == 0 || self.levels.is_empty() {
            return &self.levels[0];
        }
        let total_samples = duration_ms as f64 * self.sample_rate as f64 / 1000.0;
        let target_spp = (total_samples / pixels as f64).ceil() as u32;
        // Mức nhỏ nhất có spp >= target — đủ chi tiết mà không thừa.
        self.levels
            .iter()
            .find(|l| l.spp >= target_spp)
            .unwrap_or_else(|| self.levels.last().expect("levels không rỗng"))
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.extend_from_slice(&self.channels.to_le_bytes());
        out.extend_from_slice(&(self.levels.len() as u32).to_le_bytes());
        for l in &self.levels {
            out.extend_from_slice(&l.spp.to_le_bytes());
            out.extend_from_slice(&(l.pairs.len() as u64).to_le_bytes());
            for &(lo, hi) in &l.pairs {
                out.extend_from_slice(&lo.to_le_bytes());
                out.extend_from_slice(&hi.to_le_bytes());
            }
        }
        let crc = crc32(&out);
        out.extend_from_slice(&crc.to_le_bytes());
        out
    }

    /// Decode. File hỏng phải trả lỗi, KHÔNG panic (fuzz §10 của plan).
    pub fn decode(bytes: &[u8]) -> Result<Self, PeaksError> {
        if bytes.len() < 8 + 4 * 3 + 4 || &bytes[..8] != MAGIC {
            return Err(if bytes.len() >= 8 {
                PeaksError::BadMagic
            } else {
                PeaksError::Truncated
            });
        }
        let (body, crc_bytes) = bytes.split_at(bytes.len() - 4);
        let stored_crc = u32::from_le_bytes(crc_bytes.try_into().map_err(|_| PeaksError::Truncated)?);
        if crc32(body) != stored_crc {
            return Err(PeaksError::BadCrc);
        }
        let mut r = Reader { b: body, at: 8 };
        let sample_rate = r.u32()?;
        let channels = r.u32()?;
        let level_count = r.u32()?;
        if level_count > 16 {
            return Err(PeaksError::Invalid("số mức phi lý".into()));
        }
        let mut levels = Vec::with_capacity(level_count as usize);
        for _ in 0..level_count {
            let spp = r.u32()?;
            let count = r.u64()?;
            if count > (1 << 30) {
                return Err(PeaksError::Invalid("số cặp peak phi lý".into()));
            }
            let mut pairs = Vec::with_capacity(count as usize);
            for _ in 0..count {
                pairs.push((r.f32()?, r.f32()?));
            }
            levels.push(PeakLevel { spp, pairs });
        }
        Ok(Self {
            sample_rate,
            channels,
            levels,
        })
    }
}

/// CRC-32 (IEEE) tự triển khai — tránh thêm dependency cho một hàm 20 dòng.
fn crc32(bytes: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, e) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
        *e = c;
    }
    let mut crc = !0u32;
    for &b in bytes {
        crc = table[((crc ^ u32::from(b)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], PeaksError> {
        if self.at + n > self.b.len() {
            return Err(PeaksError::Truncated);
        }
        let s = &self.b[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, PeaksError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| PeaksError::Truncated)?,
        ))
    }
    fn u64(&mut self) -> Result<u64, PeaksError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().map_err(|_| PeaksError::Truncated)?,
        ))
    }
    fn f32(&mut self) -> Result<f32, PeaksError> {
        Ok(f32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| PeaksError::Truncated)?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_covers_all_frames() {
        // 1000 frame stereo, level 256 → ceil(1000/256) = 4 cặp.
        let samples: Vec<f32> = (0..2000).map(|i| (i as f32 / 1000.0) - 1.0).collect();
        let mm = PeakMipmap::compute(&samples, 2, 48_000).unwrap();
        assert_eq!(mm.levels[0].spp, 256);
        assert_eq!(mm.levels[0].pairs.len(), 4);
        assert_eq!(mm.levels[3].pairs.len(), 1);
    }

    #[test]
    fn roundtrip_encode_decode() {
        let samples: Vec<f32> = (0..1000).map(|i| ((i as f32) * 0.01).sin()).collect();
        let mm = PeakMipmap::compute(&samples, 1, 48_000).unwrap();
        let bytes = mm.encode();
        let back = PeakMipmap::decode(&bytes).unwrap();
        assert_eq!(mm, back);
    }

    #[test]
    fn corrupted_peaks_error_not_panic() {
        let samples: Vec<f32> = vec![0.0; 512];
        let mm = PeakMipmap::compute(&samples, 1, 48_000).unwrap();
        let mut bytes = mm.encode();
        // Xới tung một byte giữa file.
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xFF;
        assert!(matches!(PeakMipmap::decode(&bytes), Err(PeaksError::BadCrc)));
        // Cắt dở.
        let half = &bytes[..bytes.len() / 2];
        assert!(matches!(
            PeakMipmap::decode(half),
            Err(PeaksError::BadMagic | PeaksError::BadCrc | PeaksError::Truncated)
        ));
    }

    #[test]
    fn pick_level_by_zoom() {
        let samples: Vec<f32> = vec![0.0; 48_000 * 180]; // 3 phút mono
        let mm = PeakMipmap::compute(&samples, 1, 48_000).unwrap();
        // Zoom xa: 1000px cho 180s → target ≈ 8640 spp → chọn 16384.
        assert_eq!(mm.pick(180_000, 1000).spp, 16384);
        // Zoom gần: 100_000px → target ≈ 86 spp → chọn 256.
        assert_eq!(mm.pick(180_000, 100_000).spp, 256);
    }
}
