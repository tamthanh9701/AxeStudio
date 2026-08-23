//! Decoder wrapper cho symphonia — chạy CHỈ ở prefetch thread, không bao giờ
//! ở audio callback. Ở đây được phép cấp phát, I/O và mọi thứ "bẩn" khác;
//! biên sang RT path là [`super::reader::Chunk`] (boxed slice định hình xong).
//!
//! Đầu ra chuẩn hoá: **f32 interleaved theo số kênh gốc** (mono = L=R; stereo
//! = L,R,L,R,...). App chuẩn hoá asset về 48kHz lúc import (als-media), nên
//! rate khác 48kHz bị từ chối thay vì phát pitch-sai.
//!
//! Pattern decode/EOF theo `als-media/src/decode.rs` (đã chạy thật): packet
//! hỏng bỏ qua, EOF báo bằng IoError(UnexpectedEof).

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, SeekedTo};
use symphonia::core::io::MediaSource;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::TimeBase;

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("mở file thất bại: {0}")]
    Io(#[from] std::io::Error),
    #[error("symphonia: {0}")]
    Symphonia(String),
    #[error("không tìm thấy track audio trong file")]
    NoAudioTrack,
    #[error("không xác định được sample rate của track")]
    NoSampleRate,
    #[error("chỉ hỗ trợ 1–2 kênh, file có {0}")]
    UnsupportedChannels(usize),
    #[error("sample rate {0} Hz — app chuẩn hoá 48kHz lúc import, đọc thẳng sẽ pitch-sai")]
    UnexpectedSampleRate(u32),
}

impl From<SymphoniaError> for DecodeError {
    fn from(e: SymphoniaError) -> Self {
        Self::Symphonia(e.to_string())
    }
}

/// Metadata tĩnh cần cho `AudioSource`, đọc xong header là có ngay.
pub struct Opened {
    pub sample_rate: u32,
    /// Số FRAME (cặp L/R với stereo) của toàn file.
    pub total_frames: u64,
}

pub(crate) struct PacketDecoder {
    format: Box<dyn FormatReader>,
    codec: Box<dyn Decoder>,
    track_id: u32,
    time_base: TimeBase,
    channels: usize,
    /// Sau seek Coarse: số SAMPLE cần bỏ ở đầu packet kế tiếp để tới đúng
    /// frame yêu cầu (`required_ts - actual_ts` từ kết quả seek).
    pending_trim: u64,
    /// Đuôi sample chưa đủ một chunk — ghép vào lượt decode kế tiếp.
    carry: Vec<f32>,
}

pub(crate) fn open_path(path: &Path) -> Result<(PacketDecoder, Opened), DecodeError> {
    let file = File::open(path)?;
    open_source(Box::new(file))
}

/// Nhận nguồn bất kỳ (File thật, hoặc Cursor cho test in-RAM).
pub(crate) fn open_source(
    source: Box<dyn MediaSource>,
) -> Result<(PacketDecoder, Opened), DecodeError> {
    let mss = symphonia::core::io::MediaSourceStream::new(source, Default::default());
    // Extension chỉ là gợi ý probe — bỏ trống, probe tự nhận diện nội dung.
    let probed = symphonia::default::get_probe().format(
        &Hint::new(),
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or(DecodeError::NoAudioTrack)?
        .clone();
    let track_id = track.id;
    let params = &track.codec_params;

    let channels = params.channels.ok_or(DecodeError::NoAudioTrack)?.count();
    if !(1..=2).contains(&channels) {
        return Err(DecodeError::UnsupportedChannels(channels));
    }
    let sample_rate = params.sample_rate.ok_or(DecodeError::NoSampleRate)?;
    if sample_rate != 48_000 {
        return Err(DecodeError::UnexpectedSampleRate(sample_rate));
    }
    let time_base = params.time_base.ok_or(DecodeError::NoAudioTrack)?;

    // n_frames đếm THEO SAMPLE (mọi kênh gộp) — chia channels ra frame.
    let total_frames = params.n_frames.unwrap_or(0) / channels as u64;
    let opened = Opened {
        sample_rate,
        total_frames,
    };

    let codec = symphonia::default::get_codecs().make(params, &DecoderOptions::default())?;

    Ok((
        PacketDecoder {
            format,
            codec,
            track_id,
            time_base,
            channels,
            pending_trim: 0,
            carry: Vec::new(),
        },
        opened,
    ))
}

impl PacketDecoder {
    pub(crate) fn channels(&self) -> usize {
        self.channels
    }

    pub(crate) fn total_frames(&self) -> u64 {
        self.format
            .tracks()
            .iter()
            .find(|t| t.id == self.track_id)
            .and_then(|t| t.codec_params.n_frames)
            .unwrap_or(0)
            / self.channels as u64
    }

    /// Nhảy đến frame tuyệt đối. Coarse là đủ cho nghe; phần lệch tinh được
    /// trim trong [`Self::next_samples`] bằng `pending_trim`.
    pub(crate) fn seek_to(&mut self, frame: u64) -> Result<(), DecodeError> {
        let ts = frame * self.channels as u64;
        let SeekedTo {
            required_ts,
            actual_ts,
            ..
        } = self.format.seek(
            SeekMode::Coarse,
            SeekTo::Time {
                time: self.time_base.calc_time(ts),
                track_id: Some(self.track_id),
            },
        )?;
        // Đơn vị ts của WAV/FLAC = sample index (mọi kênh gộp).
        self.pending_trim = required_ts.saturating_sub(actual_ts);
        self.carry.clear();
        Ok(())
    }

    /// Trả tối đa `max_samples` sample f32 mới kể từ vị trí hiện tại.
    /// `None` = hết file. Trim sau seek xử lý ở đây nên caller luôn nhận
    /// dữ liệu BẮT ĐẦU đúng frame đã yêu cầu seek.
    pub(crate) fn next_samples(
        &mut self,
        max_samples: usize,
    ) -> Result<Option<Vec<f32>>, DecodeError> {
        let mut out = Vec::with_capacity(max_samples);

        // Ghép phần dư của lượt trước trước khi decode thêm.
        if !self.carry.is_empty() {
            let take = self.carry.len().min(max_samples);
            out.extend_from_slice(&self.carry[..take]);
            self.carry.drain(..take);
        }

        while out.len() < max_samples {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                // WAV/FLAC báo EOF bằng IoError(UnexpectedEof) — như als-media.
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break
                }
                Err(e) => return Err(e.into()),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            let decoded = match self.codec.decode(&packet) {
                Ok(d) => d,
                // Gói lẻ hỏng bỏ qua — một frame xấu không đáng hủy cả file.
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break
                }
                Err(e) => return Err(e.into()),
            };
            let spec = *decoded.spec();
            let n_samples = decoded.capacity() as u64;

            let mut buf = SampleBuffer::<f32>::new(n_samples, spec);
            buf.copy_interleaved_ref(decoded);
            let samples_all: &[f32] = buf.samples();

            // Trim sau seek Coarse: bỏ sample thuộc trước vị trí yêu cầu.
            let mut samples = samples_all;
            if self.pending_trim > 0 {
                let skip = (self.pending_trim as usize).min(samples.len());
                samples = &samples[skip..];
                self.pending_trim -= skip as u64;
            }

            let room = max_samples - out.len();
            let take = samples.len().min(room);
            out.extend_from_slice(&samples[..take]);
            if take < samples.len() {
                self.carry.extend_from_slice(&samples[take..]);
            }
            if take < samples.len() {
                // Đã đủ max_samples — thoát vòng, phần còn lại ở carry/lần sau.
                break;
            }
        }

        if out.is_empty() {
            Ok(None)
        } else {
            Ok(Some(out))
        }
    }
}
