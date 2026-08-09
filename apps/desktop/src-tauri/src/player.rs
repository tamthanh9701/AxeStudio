//! Playback wiring: arrangement → engine sources.
//!
//! Chiến lược v1 — CONSOLIDATION: mỗi track được render trước thành MỘT buffer
//! timeline-absolute (clip đặt đúng start_ms, cắt theo offset_ms, nhân gain,
//! overlap cộng dồn). Engine chỉ phát N buffer với gain/pan/mute/solo — đúng
//! thế mạnh của als-audio hiện tại, không cần scheduler per-clip.
//!
//! Cái giá: đổi take/edit → rebuild engine (click ~50ms), chưa stream file dài.
//! TODO(S2): streaming per-clip + live swap bằng arc-swap, không rebuild.

use crate::state::AppState;
use als_assets::AssetStore;
use als_audio::{db_to_linear, AudioConfig, AudioSource, BufferSource, EngineBuilder};
use als_core::{AssetId, ClipSource, ErrorCode, ExportRange, ExportSpec, IpcError, Track};
use als_media::AudioBuffer;
use als_project::{Db, Project};
use tauri::{Emitter, State};

const SR: u64 = 48_000;
/// Chặn RAM: 5 phút @48k stereo f32 ≈ 115MB/track. Vượt → cắt, S2 stream thật.
const MAX_CONSOLIDATE_MS: u64 = 5 * 60 * 1000;

fn to_frames(ms: u64) -> usize {
    (ms * SR / 1000) as usize
}

/// Giải mã audio của một clip.
///
/// - `Ok(Some(..))` — buffer 48k + gain tuyến tính.
/// - `Ok(None)` — clip hợp lệ nhưng chưa có tiếng (Generated chưa có take
///   active). Im lặng ở đây là đúng, không phải lỗi.
/// - `Err(..)` — lỗi THẬT (db/decode/resample). C3: trước đây 4 chỗ `.ok()?`
///   liên tiếp nuốt sạch mọi lỗi thành "clip im lặng" — không log, không
///   event, không cách nào debug.
fn clip_audio(
    clip: &als_core::Clip,
    db: &Db,
    store: &AssetStore,
) -> Result<Option<(AudioBuffer, f32)>, String> {
    let asset_id = match &clip.source {
        ClipSource::Imported { asset } => asset.clone(),
        ClipSource::Generated => {
            let Some(take_id) = clip.active_take.as_ref() else {
                return Ok(None);
            };
            let takes = db
                .takes_for_clip(clip.id.as_str())
                .map_err(|e| format!("đọc take của clip: {e}"))?;
            let take = takes
                .iter()
                .find(|t| t.id == take_id.as_str())
                .ok_or_else(|| format!("active take {} không có trong db", take_id.as_str()))?;
            AssetId::from(take.asset_id.clone())
        }
    };
    let asset = db
        .asset_get(&asset_id)
        .map_err(|e| format!("đọc asset {}: {e}", asset_id.as_str()))?
        .ok_or_else(|| format!("asset {} không có trong db", asset_id.as_str()))?;
    let path = store.abs_path(&asset.rel_path);
    let buf = als_media::decode::decode_file(&path)
        .map_err(|e| format!("decode {}: {e}", asset.rel_path))?;
    // C4: KHÔNG được fallback về buffer gốc khi resample lỗi — engine luôn chạy
    // 48kHz, nên phát thẳng buffer 44.1kHz sẽ nhanh hơn ~8.8% và cao hơn ~1.5
    // semitone. Im lặng + báo lỗi còn đỡ hại hơn phát sai mà người dùng tưởng
    // model sinh hỏng.
    let buf = als_media::resample::to_target_rate(&buf)
        .map_err(|e| format!("resample {}: {e}", asset.rel_path))?;
    Ok(Some((buf, db_to_linear(clip.gain_db))))
}

/// Render một track thành buffer stereo timeline-absolute.
/// Trả kèm danh sách cảnh báo: clip nào bị bỏ qua vì lỗi gì (C3 — trước đây
/// chỉ `continue` âm thầm).
fn consolidate_track(track: &Track, db: &Db, store: &AssetStore) -> (BufferSource, Vec<String>) {
    let end_ms = track
        .clips
        .iter()
        .map(|c| c.start_ms + c.duration_ms)
        .max()
        .unwrap_or(0)
        .min(MAX_CONSOLIDATE_MS);
    let total_frames = to_frames(end_ms).max(1);
    let mut data = vec![0.0f32; total_frames * 2];
    let mut warnings = Vec::new();

    for clip in &track.clips {
        let (buf, gain) = match clip_audio(clip, db, store) {
            Ok(Some(v)) => v,
            Ok(None) => continue,
            Err(reason) => {
                warnings.push(format!("clip {}: {reason}", clip.id.as_str()));
                continue;
            }
        };
        let ch = buf.channels.max(1) as usize;
        let offset = to_frames(clip.offset_ms).min(buf.frames());
        let start = to_frames(clip.start_ms);
        let want = to_frames(clip.duration_ms)
            .min(buf.frames().saturating_sub(offset))
            .min(total_frames.saturating_sub(start));
        for f in 0..want {
            let si = (offset + f) * ch;
            let di = (start + f) * 2;
            let l = buf.samples.get(si).copied().unwrap_or(0.0);
            // Stereo: R ở si+1. Mono: R = L.
            let r = if ch >= 2 {
                buf.samples.get(si + 1).copied().unwrap_or(0.0)
            } else {
                l
            };
            let (Some(dl), Some(dr)) = (data.get_mut(di), data.get_mut(di + 1)) else {
                continue;
            };
            *dl += l * gain;
            *dr += r * gain;
        }
    }
    (BufferSource::from_interleaved(data), warnings)
}

/// Đọc arrangement hiện tại → sources mới → REBUILD engine. Gọi sau
/// project_open/create, take_promote, take:ready, edit.
///
/// C1: engine mới khởi tạo `Transport::default()` → position = 0, loop tắt.
/// Vì vậy trước khi thay engine cũ phải đọc playhead, và sau khi dựng engine
/// mới phải replay `Seek` + `SetLoop` (loop lưu ở `AppState.loop_region`) —
/// nếu không, mỗi lần chỉnh gain/mute/edit là nhạc nhảy về 0:00 và mất loop.
pub async fn refresh(state: &State<'_, AppState>) -> Result<(), IpcError> {
    let (tracks, layout) = {
        let guard = state.project.lock().await;
        let Some(p) = guard.as_ref() else {
            return Ok(());
        };
        (p.manifest.arrangement.tracks.clone(), p.layout.clone())
    };

    let was_playing = state.playing.load(std::sync::atomic::Ordering::Relaxed);
    // Vị trí + loop của engine cũ — sẽ replay vào engine mới (C1).
    let resume_frames = {
        let guard = state.engine.lock().await;
        guard
            .as_ref()
            .map(|e| e.playhead().load_frames())
            .unwrap_or(0)
    };
    let loop_region = *state.loop_region.lock().await;

    let tracks_c = tracks.clone();
    let layout_c = layout.clone();
    let (mut engine, warnings) = tokio::task::spawn_blocking(move || -> Result<_, IpcError> {
        let store = AssetStore::new(layout_c.assets_dir())
            .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
        let db = Db::open(&layout_c.db_path())
            .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
        let mut builder = EngineBuilder::new(AudioConfig::default());
        let mut warnings = Vec::new();
        for track in &tracks_c {
            let (src, mut w) = consolidate_track(track, &db, &store);
            warnings.append(&mut w);
            builder = builder.with_source(Box::new(src));
        }
        let engine = builder.start().map_err(|e| {
            IpcError::new(
                ErrorCode::ProviderUnavailable,
                format!("không mở được thiết bị audio: {e}"),
            )
            .retryable(true)
        })?;
        Ok((engine, warnings))
    })
    .await
    .map_err(|e| IpcError::new(ErrorCode::Internal, format!("player task: {e}")))??;

    // C3: lỗi clip không còn bị nuốt — log từng clip + emit MỘT event tổng hợp
    // để UI hiện cảnh báo "N clip im lặng do lỗi" (UI subscribe event này sau).
    if !warnings.is_empty() {
        for w in &warnings {
            tracing::warn!("consolidate bỏ qua {}", w);
        }
        let _ = state.handle().emit(
            "clip:audio_error",
            serde_json::json!({ "count": warnings.len(), "messages": warnings }),
        );
    }

    // Đẩy lại gain/pan/mute/solo từ arrangement (mixer state không sống qua rebuild).
    for (i, track) in tracks.iter().enumerate() {
        let t = i as u8;
        engine.send_command(als_audio::Command::SetTrackGain {
            track: t,
            gain: db_to_linear(track.gain_db),
        });
        engine.send_command(als_audio::Command::SetTrackPan {
            track: t,
            pan: track.pan,
        });
        engine.send_command(als_audio::Command::SetTrackMute {
            track: t,
            mute: track.mute,
        });
        engine.send_command(als_audio::Command::SetTrackSolo {
            track: t,
            solo: track.solo,
        });
    }
    // C1: replay vị trí phát + vùng loop TRƯỚC khi play lại.
    if resume_frames > 0 {
        engine.send_command(als_audio::Command::Seek(resume_frames));
    }
    if let Some((start_ms, end_ms)) = loop_region {
        engine.set_loop(start_ms, end_ms, true);
    }
    if was_playing {
        engine.play();
    }
    *state.engine.lock().await = Some(engine);
    Ok(())
}

/// Bounce offline toàn project (hoặc vùng loop) → WAV 24-bit.
/// Dùng CHÍNH Mixer của als-audio — đường render nghe và xuất là một.
pub fn bounce(
    layout: &crate::state::Layout,
    tracks: &[Track],
    spec: &ExportSpec,
    app_version: &str,
) -> Result<String, IpcError> {
    use als_audio::{AudioSource, Mixer};
    use als_core::ExportFormat;

    if spec.format != ExportFormat::Wav24 {
        return Err(IpcError::new(
            ErrorCode::CapabilityNotSupported,
            "MP3/FLAC cần FFmpeg sidecar — dự kiến Sprint 6; v1 xuất WAV 24-bit",
        ));
    }

    let store = AssetStore::new(layout.assets_dir())
        .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
    let db = Db::open(&layout.db_path())
        .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;

    let mut mixer = Mixer::new();
    let mut sources: Vec<Option<Box<dyn AudioSource>>> = Vec::with_capacity(tracks.len());
    for track in tracks {
        let (src, warnings) = consolidate_track(track, &db, &store);
        for w in &warnings {
            tracing::warn!("bounce bỏ qua {}", w);
        }
        sources.push(Some(Box::new(src)));
        let idx = mixer.add_track().expect("tối đa 32 track");
        let st = &mut mixer.tracks[idx];
        st.set_gain(db_to_linear(track.gain_db));
        st.set_pan(track.pan);
        st.mute = track.mute;
        st.solo = track.solo;
    }

    // Vùng render.
    let project_end_ms = tracks
        .iter()
        .flat_map(|t| t.clips.iter().map(|c| c.start_ms + c.duration_ms))
        .max()
        .unwrap_or(0)
        .min(MAX_CONSOLIDATE_MS);
    let (start_ms, end_ms) = match &spec.range {
        ExportRange::Project => (0, project_end_ms),
        ExportRange::Loop { start_ms, end_ms } => (*start_ms, (*end_ms).min(project_end_ms)),
    };
    if end_ms <= start_ms {
        return Err(IpcError::new(
            ErrorCode::ExportFailed,
            "vùng export rỗng — project chưa có audio hoặc loop không hợp lệ",
        ));
    }

    // Seek nguồn tới đầu vùng rồi render từng block 512 frame như realtime.
    let start_frames = to_frames(start_ms) as u64;
    for s in sources.iter_mut().flatten() {
        s.seek_frames(start_frames);
    }
    let total = to_frames(end_ms - start_ms);
    let mut pcm: Vec<f32> = Vec::with_capacity(total * 2);
    let mut block = [0.0f32; 1024];
    while pcm.len() < total * 2 {
        let remain = total * 2 - pcm.len();
        let n = remain.min(block.len());
        mixer.render(&mut sources, &mut block[..n]);
        pcm.extend_from_slice(&block[..n]);
    }

    // WAV 24-bit PCM.
    let out_path = std::path::Path::new(&spec.out_path);
    if let Some(dir) = out_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
    }
    let wav_spec = hound::WavSpec {
        channels: 2,
        sample_rate: SR as u32,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(out_path, wav_spec)
        .map_err(|e| IpcError::new(ErrorCode::ExportFailed, e.to_string()))?;
    for &s in &pcm {
        let v = (s.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
        w.write_sample(v)
            .map_err(|e| IpcError::new(ErrorCode::ExportFailed, e.to_string()))?;
    }
    w.finalize()
        .map_err(|e| IpcError::new(ErrorCode::ExportFailed, e.to_string()))?;

    // Sidecar metadata + disclosure (plan S6).
    if spec.include_metadata {
        let meta = serde_json::json!({
            "app": "AxeStudio",
            "version": app_version,
            "disclosure": "Contains AI-generated audio (ACE-Step)",
            "exported_at_unix": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            "range_ms": { "start": start_ms, "end": end_ms },
            "format": "wav24",
        });
        let sidecar = out_path.with_extension("meta.json");
        if let Ok(json) = serde_json::to_string_pretty(&meta) {
            let _ = std::fs::write(sidecar, json);
        }
    }

    Ok(spec.out_path.clone())
}
