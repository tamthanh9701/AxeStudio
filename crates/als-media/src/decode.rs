//! Decode file audio → AudioBuffer f32 interleaved qua symphonia (Rust thuần).
//! File hỏng / format lạ → Err, KHÔNG panic (fuzz §10).

use crate::{AudioBuffer, MediaError};
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub fn decode_file(path: &Path) -> Result<AudioBuffer, MediaError> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| MediaError::Decode(format!("probe: {e}")))?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.sample_rate.is_some())
        .cloned()
        .ok_or(MediaError::NoAudioTrack)?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(48_000);
    let channels = track
        .codec_params
        .channels
        .map(|c| c.count() as u32)
        .unwrap_or(2);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| MediaError::Decode(format!("codec: {e}")))?;

    let mut samples: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(e) => return Err(MediaError::Decode(format!("packet: {e}"))),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                buf.copy_interleaved_ref(decoded);
                samples.extend_from_slice(buf.samples());
            }
            // Gói lẻ hỏng thì bỏ qua — một frame xấu không đáng hủy cả file.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(e) => return Err(MediaError::Decode(format!("decode: {e}"))),
        }
    }

    Ok(AudioBuffer {
        samples,
        sample_rate,
        channels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_sine_wav(path: &Path, sr: u32, secs: f32) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let frames = (sr as f32 * secs) as usize;
        for i in 0..frames {
            let s = (2.0 * std::f32::consts::PI * 440.0 * (i as f32 / sr as f32)).sin() * 0.1;
            w.write_sample(s).unwrap();
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn decodes_wav_f32() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        write_sine_wav(&path, 48_000, 0.5);
        let buf = decode_file(&path).unwrap();
        assert_eq!(buf.sample_rate, 48_000);
        assert_eq!(buf.channels, 2);
        // Cho phép lệch vài frame do container padding.
        let frames = buf.frames();
        assert!(
            (24_000i64 - frames as i64).abs() < 64,
            "frames = {frames}, kỳ vọng ≈ 24000"
        );
    }

    #[test]
    fn corrupt_file_errors_not_panics() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.wav");
        std::fs::write(&path, b"RIFF\x00\x00broken").unwrap();
        assert!(decode_file(&path).is_err());
    }
}
