//! LUFS + true-peak theo ITU-R BS.1770 qua ebur128.
//! Chạy ở postprocess sau render — KHÔNG nằm trên đường realtime.

use crate::{AudioBuffer, MediaError};
use ebur128::{EbuR128, Mode};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loudness {
    /// Integrated loudness (LUFS). -70.0 khi im lặng tuyệt đối (gate floor).
    pub lufs: f64,
    /// True peak đỉnh toàn kênh, dBTP.
    pub true_peak_db: f64,
}

pub fn measure(buf: &AudioBuffer) -> Result<Loudness, MediaError> {
    if buf.channels == 0 || buf.samples.is_empty() {
        return Err(MediaError::Loudness("buffer rỗng".into()));
    }
    let mut e = EbuR128::new(buf.channels, buf.sample_rate, Mode::I | Mode::TRUE_PEAK)
        .map_err(|err| MediaError::Loudness(err.to_string()))?;
    e.add_frames_f32(&buf.samples)
        .map_err(|err| MediaError::Loudness(err.to_string()))?;

    let mut lufs = e
        .loudness_global()
        .map_err(|err| MediaError::Loudness(err.to_string()))?;
    // ebur128 trả -inf cho silence; JSON không biểu diễn được -inf → gate ở -70.
    if !lufs.is_finite() {
        lufs = -70.0;
    }

    let mut tp_linear = 0.0f64;
    for ch in 0..buf.channels {
        let tp = e
            .true_peak(ch)
            .map_err(|err| MediaError::Loudness(err.to_string()))?;
        tp_linear = tp_linear.max(tp);
    }
    let true_peak_db = if tp_linear > 0.0 {
        20.0 * tp_linear.log10()
    } else {
        -144.0
    };

    Ok(Loudness {
        lufs,
        true_peak_db,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(amp: f32, secs: f32) -> AudioBuffer {
        let sr = 48_000u32;
        let frames = (sr as f32 * secs) as usize;
        let mut samples = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let s = (2.0 * std::f32::consts::PI * 1000.0 * (i as f32 / sr as f32)).sin() * amp;
            samples.push(s);
            samples.push(s);
        }
        AudioBuffer {
            samples,
            sample_rate: sr,
            channels: 2,
        }
    }

    #[test]
    fn tone_loudness_in_expected_range() {
        // Sine 1 kHz biên độ 0.5, STEREO (hai kênh giống nhau):
        //   MS mỗi kênh = 0.5²/2 = 0.125        → 10·log10 = -9.03 dB
        //   BS.1770 CỘNG năng lượng các kênh (L và R hệ số 1.0): 0.25 → -6.02 dB
        //   LUFS = -0.691 + (-6.02) + gain K-weight tại 1 kHz (≈ +0.70 dB) ≈ -6.01
        // Đo thực tế: -6.0139 — implementation đúng.
        // (Bản cũ kỳ vọng ≈ -16 vì tính mono — quên cộng kênh — và RMS nhầm thành
        // -13 dBFS; tổng sai ~10 dB. Lỗi của TEST, không phải của measure.)
        let l = measure(&tone(0.5, 2.0)).unwrap();
        assert!((-6.5..=-5.5).contains(&l.lufs), "lufs = {}", l.lufs);
        // True peak ≈ 0.5 → ≈ -6 dBTP, cho phép ±1.5.
        assert!(
            (l.true_peak_db - (-6.02)).abs() < 1.5,
            "tp = {}",
            l.true_peak_db
        );
    }

    #[test]
    fn halving_amplitude_drops_lufs_6db() {
        // Bất biến tương đối không phụ thuộc constant -0.691 hay gain K-weight:
        // giảm biên độ một nửa → năng lượng giảm 4 lần → LUFS giảm đúng 6.02 LU.
        let a = measure(&tone(0.5, 2.0)).unwrap();
        let b = measure(&tone(0.25, 2.0)).unwrap();
        assert!(
            (a.lufs - b.lufs - 6.0206).abs() < 0.1,
            "delta = {}",
            a.lufs - b.lufs
        );
    }

    #[test]
    fn silence_is_gated_not_infinite() {
        let l = measure(&tone(0.0, 1.0)).unwrap();
        assert!(l.lufs.is_finite());
        assert!(l.lufs <= -70.0);
    }
}
