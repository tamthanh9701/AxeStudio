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
    let mut e = EbuR128::new(
        buf.channels,
        buf.sample_rate,
        Mode::I | Mode::TRUE_PEAK,
    )
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
        // Sine 1kHz biên độ 0.5: RMS ≈ -13 dBFS, K-weighted ≈ -16 LUFS, cho phép ±4.
        let l = measure(&tone(0.5, 2.0)).unwrap();
        assert!((-20.0..=-10.0).contains(&l.lufs), "lufs = {}", l.lufs);
        // True peak ≈ 0.5 → ≈ -6 dBTP, cho phép ±1.5.
        assert!(
            (l.true_peak_db - (-6.02)).abs() < 1.5,
            "tp = {}",
            l.true_peak_db
        );
    }

    #[test]
    fn silence_is_gated_not_infinite() {
        let l = measure(&tone(0.0, 1.0)).unwrap();
        assert!(l.lufs.is_finite());
        assert!(l.lufs <= -70.0);
    }
}
