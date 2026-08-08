//! Import asset: decode → normalize 48kHz → encode WAV f32 → store → db → peaks.
//!
//! Chạy NGOÀI mọi lock của AppState (decode 1GB file không được chặn IPC khác).
//! Ghi db qua connection THỨ HAI — busy_timeout=5000 đã set trong Db::open.

use als_assets::{AssetStore, PeakMipmap};
use als_core::{AssetId, ErrorCode, IpcError};
use als_media::{AudioBuffer, MediaError};
use als_project::{AssetRow, ProjectLayout};
use std::path::Path;

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn media_err(e: MediaError) -> IpcError {
    IpcError::new(ErrorCode::Internal, format!("media: {e}"))
}

/// Encode AudioBuffer (48k f32 interleaved) → WAV bytes.
pub fn encode_wav_f32(buf: &AudioBuffer) -> Result<Vec<u8>, IpcError> {
    let spec = hound::WavSpec {
        channels: buf.channels as u16,
        sample_rate: buf.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut cursor, spec)
            .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
        for &s in &buf.samples {
            w.write_sample(s)
                .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
        }
        w.finalize()
            .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
    }
    Ok(cursor.into_inner())
}

/// Import một nhóm file. File lỗi → fail cả batch kèm tên file trong message
/// (MVP; Phase 2 đổi sang per-file result khi UI có import dialog xịn).
pub fn import_files(layout: &ProjectLayout, paths: &[String]) -> Result<Vec<AssetId>, IpcError> {
    let store = AssetStore::new(layout.assets_dir())
        .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
    let db = als_project::Db::open(&layout.db_path())
        .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;

    let mut ids = Vec::with_capacity(paths.len());
    for p in paths {
        let buf = als_media::decode::decode_file(Path::new(p)).map_err(|e| {
            IpcError::new(
                ErrorCode::Internal,
                format!("không decode được {}: {e}", Path::new(p).display()),
            )
        })?;
        let buf = als_media::resample::to_target_rate(&buf).map_err(media_err)?;
        let bytes = encode_wav_f32(&buf)?;
        let (id, rel) = store
            .put(&bytes, "wav")
            .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
        db.asset_put(&AssetRow {
            id: id.to_string(),
            kind: "import".into(),
            rel_path: rel.clone(),
            bytes: bytes.len() as i64,
            sample_rate: Some(48_000),
            channels: Some(i64::from(buf.channels)),
            duration_ms: Some(buf.duration_ms() as i64),
            created_at: now_unix(),
        })
        .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;

        // Peaks ngay trong lượt import — contract nói async, nhưng với peaks
        // < 800ms/3 phút audio thì sync ở đây đơn giản hơn mà vẫn trong budget.
        // Nếu bench S3 chứng minh chậm, chuyển sang task nền + event peaks:ready.
        if let Ok(mm) = PeakMipmap::compute(&buf.samples, buf.channels, buf.sample_rate) {
            if let Ok(peak_rel) = AssetStore::rel_path(&id, "alspeak") {
                let _ = store.put_named(&peak_rel, &mm.encode());
            }
        }
        ids.push(id);
    }
    Ok(ids)
}
