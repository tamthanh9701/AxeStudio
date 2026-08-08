//! Resample về 48kHz — chuẩn của toàn hệ thống (plan §5).
//! 48k đầu vào → passthrough, không đụng vào mẫu.

use crate::{AudioBuffer, MediaError};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

pub const TARGET_RATE: u32 = 48_000;

pub fn to_target_rate(buf: &AudioBuffer) -> Result<AudioBuffer, MediaError> {
    if buf.sample_rate == TARGET_RATE {
        return Ok(buf.clone());
    }
    if buf.sample_rate == 0 || buf.channels == 0 || buf.samples.is_empty() {
        return Err(MediaError::Resample("buffer rỗng hoặc rate = 0".into()));
    }

    let ratio = TARGET_RATE as f64 / buf.sample_rate as f64;
    let channels = buf.channels as usize;
    let frames = buf.frames();

    // Deinterleave.
    let mut in_ch: Vec<Vec<f32>> = vec![Vec::with_capacity(frames); channels];
    for (i, &s) in buf.samples.iter().enumerate() {
        in_ch[i % channels].push(s);
    }

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let chunk = 4096usize;
    let mut rs = SincFixedIn::new(ratio, 1.0, params, chunk, channels)
        .map_err(|e| MediaError::Resample(e.to_string()))?;

    let mut out_ch: Vec<Vec<f32>> = vec![Vec::new(); channels];
    let mut offset = 0;
    while offset < frames {
        let end = (offset + chunk).min(frames);
        let mut block: Vec<Vec<f32>> = in_ch
            .iter()
            .map(|c| c[offset..end].to_vec())
            .collect();
        // Pad chunk cuối bằng zero cho đủ chunk_size, trim theo ratio sau.
        let short = end - offset;
        if short < chunk {
            for c in block.iter_mut() {
                c.resize(chunk, 0.0);
            }
        }
        let processed = rs
            .process(&block, None)
            .map_err(|e| MediaError::Resample(e.to_string()))?;
        let keep = ((short as f64 * ratio).round() as usize).min(processed[0].len());
        for (dst, src) in out_ch.iter_mut().zip(processed.iter()) {
            dst.extend_from_slice(&src[..keep]);
        }
        offset = end;
    }

    // Interleave lại.
    let out_frames = out_ch[0].len();
    let mut samples = Vec::with_capacity(out_frames * channels);
    for f in 0..out_frames {
        for c in out_ch.iter() {
            samples.push(c[f]);
        }
    }

    Ok(AudioBuffer {
        samples,
        sample_rate: TARGET_RATE,
        channels: buf.channels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_at_48k() {
        let buf = AudioBuffer {
            samples: vec![0.1, -0.1, 0.2, -0.2],
            sample_rate: 48_000,
            channels: 2,
        };
        let out = to_target_rate(&buf).unwrap();
        assert_eq!(out.samples, buf.samples, "48k vào phải nguyên vẹn");
    }

    #[test]
    fn resamples_441_to_48() {
        let sr_in = 44_100u32;
        let frames = 44_100; // 1 giây
        let mut samples = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let s = (2.0 * std::f32::consts::PI * 440.0 * (i as f32 / sr_in as f32)).sin() * 0.1;
            samples.push(s);
            samples.push(s);
        }
        let buf = AudioBuffer {
            samples,
            sample_rate: sr_in,
            channels: 2,
        };
        let out = to_target_rate(&buf).unwrap();
        assert_eq!(out.sample_rate, 48_000);
        // 1 giây @44.1k → 48000 frame, cho phép lệch 1% do pad/trim.
        let frames_out = out.frames() as i64;
        assert!(
            (48_000i64 - frames_out).abs() < 480,
            "frames_out = {frames_out}"
        );
    }
}
