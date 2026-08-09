//! Toàn bộ IPC command — khớp docs/contracts/ipc.md.
//! Mọi lỗi trả về IpcError (code enum đóng); UI map code → tiếng Việt.
//!
//! Mỗi command PHẢI có đủ CẢ HAI attribute:
//!   #[tauri::command]   — để Tauri sinh wrapper IPC
//!   #[specta::specta]   — để tauri_specta::collect_commands! lấy được kiểu
//! Thiếu cái thứ hai thì collect_commands! trong lib.rs không compile.
//! `State<'_, AppState>` được specta bỏ qua, không xuất hiện trong bindings.

use crate::assets_io;
use crate::player;
use crate::state::AppState;
use als_assets::{AssetStore, PeakMipmap};
use als_core::{
    priority as job_priority, AssetId, ClipId, EditCommand, EditResult, EngineStatus, ErrorCode,
    ExportSpec, GenerationRecipe, IpcError, JobId, ProjectSnapshot, ProviderId, TakeId, TakeInfo,
    UndoStack,
};
use als_orchestrator::{OrchestratorHandle, OrchEvent};
use als_project::Project;
use als_provider::CancelOutcome;
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};

pub type CmdResult<T> = Result<T, IpcError>;

fn no_project() -> IpcError {
    IpcError::new(ErrorCode::ProjectNotFound, "chưa mở project")
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct EditOutcome {
    pub edit: EditResult,
    pub snapshot: ProjectSnapshot,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct UndoOutcome {
    pub label: Option<String>,
    pub snapshot: Option<ProjectSnapshot>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct TransportPosition {
    pub frames: u64,
    pub playing: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PeakView {
    pub spp: u32,
    pub pairs: Vec<(f32, f32)>,
}

/// Metadata asset cho UI (đặt clip import theo độ dài thật).
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AssetInfo {
    pub id: String,
    pub kind: String,
    pub duration_ms: Option<u64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
}

/// Row của bảng `asset` — als-project chỉ export `AssetRow` (không có
/// `AssetRecord`). SQLite không có unsigned int nên mọi số ở db đều là i64;
/// clamp `.max(0)` trước khi cast để giá trị âm (do bug ghi liều) không wrap
/// thành số khủng làm UI vẽ clip dài vô hạn.
impl From<als_project::AssetRow> for AssetInfo {
    fn from(a: als_project::AssetRow) -> Self {
        Self {
            id: a.id,
            kind: a.kind,
            duration_ms: a.duration_ms.map(|v| v.max(0) as u64),
            sample_rate: a.sample_rate.map(|v| v.max(0) as u32),
            channels: a.channels.map(|v| v.max(0) as u32),
        }
    }
}

// ---------- nội bộ ----------

fn default_providers(assets_root: &Path) -> Vec<Arc<dyn als_provider::RenderProvider>> {
    let root = assets_root.to_path_buf();
    let resolver: Arc<als_provider_py::AssetResolver> = Arc::new(move |id: &AssetId| {
        AssetStore::rel_path(id, "wav").ok().map(|rel| root.join(rel))
    });
    vec![
        Arc::new(als_provider::MockProvider::new()),
        Arc::new(als_provider_cpp::CppProvider::new("http://127.0.0.1:8080")),
        Arc::new(als_provider_py::PyProvider::new(
            "http://127.0.0.1:8001",
            None,
            resolver,
        )),
    ]
}

/// Mở session cho project vừa create/open: orchestrator (connection db thứ
/// hai) + forward event về UI + reset undo stack + nạp playback sources.
async fn bootstrap_session(state: &State<'_, AppState>, project: Project) -> CmdResult<()> {
    let orch_db = als_project::Db::open(&project.layout.db_path())?;
    let assets = AssetStore::new(project.layout.assets_dir())
        .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
    let providers = default_providers(&project.layout.assets_dir());
    // Mock là default cho tới khi Phase 0 chốt backend (ADR-001). Đổi qua
    // engine_switch_backend hoặc config first-run ở S7.
    let handle = als_orchestrator::spawn(
        orch_db,
        assets,
        providers,
        ProviderId(ProviderId::MOCK.to_owned()),
    )?;
    forward_events(state.handle().clone(), handle.clone());
    *state.orchestrator.lock().await = Some(handle);
    *state.undo.lock().await = UndoStack::new();
    *state.project.lock().await = Some(project);
    // Nạp audio của project vào engine — lỗi ở đây KHÔNG chặn mở project
    // (máy không có thiết bị audio vẫn phải dùng được phần còn lại).
    let _ = player::refresh(state).await;
    Ok(())
}

fn forward_events(app: tauri::AppHandle, handle: OrchestratorHandle) {
    let mut rx = handle.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            let ev = match rx.recv().await {
                Ok(ev) => ev,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "UI event lagged — bỏ qua một số event");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            match ev {
                OrchEvent::JobState {
                    job_id,
                    state,
                    error,
                } => {
                    let _ = app.emit(
                        "job:state",
                        serde_json::json!({
                            "job_id": job_id.to_string(),
                            "state": state,
                            "error": error,
                        }),
                    );
                }
                OrchEvent::Progress(p) => {
                    let _ = app.emit(
                        "job:progress",
                        serde_json::json!({
                            "job_id": p.job_id.to_string(),
                            "percent": p.percent,
                            "stage": p.stage,
                        }),
                    );
                }
                OrchEvent::TakeReady {
                    job_id,
                    clip_id,
                    take_id,
                    cached,
                } => {
                    let _ = app.emit(
                        "take:ready",
                        serde_json::json!({
                            "job_id": job_id.to_string(),
                            "clip_id": clip_id,
                            "take_id": take_id.to_string(),
                            "cached": cached,
                        }),
                    );
                    // Take mới → playback phải nạp lại để Play nghe được ngay.
                    let st = app.state::<AppState>();
                    let _ = player::refresh(&st).await;
                }
                OrchEvent::PeaksReady { asset_id } => {
                    let _ = app.emit(
                        "peaks:ready",
                        serde_json::json!({ "asset_id": asset_id.to_string() }),
                    );
                }
            }
        }
    });
}

async fn apply_edit_inner(state: &State<'_, AppState>, cmd: EditCommand) -> CmdResult<EditOutcome> {
    let mut project_guard = state.project.lock().await;
    let project = project_guard.as_mut().ok_or_else(no_project)?;
    let mut undo = state.undo.lock().await;
    let edit = undo.apply(&mut project.manifest.arrangement, &cmd)?;
    project.save()?;
    let dirty = undo.undo_len() > 0;
    let snapshot = project.snapshot(dirty);
    drop(undo);
    drop(project_guard);
    let _ = state
        .handle()
        .emit("project:dirty", serde_json::json!({ "dirty": dirty }));
    Ok(EditOutcome { edit, snapshot })
}

// ---------- project ----------

#[tauri::command]
#[specta::specta]
pub async fn project_create(
    state: State<'_, AppState>,
    path: String,
    name: String,
) -> CmdResult<ProjectSnapshot> {
    let project = Project::create(Path::new(&path), &name, env!("CARGO_PKG_VERSION"))?;
    let snapshot = project.snapshot(false);
    bootstrap_session(&state, project).await?;
    Ok(snapshot)
}

#[tauri::command]
#[specta::specta]
pub async fn project_open(state: State<'_, AppState>, path: String) -> CmdResult<ProjectSnapshot> {
    let project = Project::open(Path::new(&path))?;
    let snapshot = project.snapshot(false);
    bootstrap_session(&state, project).await?;
    Ok(snapshot)
}

#[tauri::command]
#[specta::specta]
pub async fn project_save_as(
    state: State<'_, AppState>,
    path: String,
) -> CmdResult<ProjectSnapshot> {
    let dst = Path::new(&path);
    if dst.exists() {
        return Err(IpcError::new(
            ErrorCode::ProjectCorrupt,
            "đường dẫn đích đã tồn tại",
        ));
    }
    let src = {
        let mut guard = state.project.lock().await;
        let project = guard.as_mut().ok_or_else(no_project)?;
        project.save()?;
        project.layout.root().to_path_buf()
    };
    copy_dir_all(&src, dst)?;
    // Mở lại tại vị trí mới — session cũ bị thay thế hoàn toàn.
    project_open(state, path).await
}

fn copy_dir_all(src: &Path, dst: &Path) -> CmdResult<()> {
    std::fs::create_dir_all(dst).map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
    for entry in std::fs::read_dir(src).map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))? {
        let entry = entry.map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
        }
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn project_apply_edit(
    state: State<'_, AppState>,
    cmd: EditCommand,
) -> CmdResult<EditOutcome> {
    let out = apply_edit_inner(&state, cmd).await?;
    // Edit ảnh hưởng arrangement (move/trim/split/remove/take) → playback nạp lại.
    let _ = player::refresh(&state).await;
    Ok(out)
}

#[tauri::command]
#[specta::specta]
pub async fn project_undo(state: State<'_, AppState>) -> CmdResult<UndoOutcome> {
    let mut project_guard = state.project.lock().await;
    let project = project_guard.as_mut().ok_or_else(no_project)?;
    let mut undo = state.undo.lock().await;
    let label = undo.undo(&mut project.manifest.arrangement);
    if label.is_some() {
        project.save()?;
    }
    let dirty = undo.undo_len() > 0;
    let snapshot = label.as_ref().map(|_| project.snapshot(dirty));
    drop(undo);
    drop(project_guard);
    let _ = state
        .handle()
        .emit("project:dirty", serde_json::json!({ "dirty": dirty }));
    if label.is_some() {
        let _ = player::refresh(&state).await;
    }
    Ok(UndoOutcome { label, snapshot })
}

#[tauri::command]
#[specta::specta]
pub async fn project_redo(state: State<'_, AppState>) -> CmdResult<UndoOutcome> {
    let mut project_guard = state.project.lock().await;
    let project = project_guard.as_mut().ok_or_else(no_project)?;
    let mut undo = state.undo.lock().await;
    let label = undo.redo(&mut project.manifest.arrangement);
    if label.is_some() {
        project.save()?;
    }
    let dirty = undo.undo_len() > 0;
    let snapshot = label.as_ref().map(|_| project.snapshot(dirty));
    drop(undo);
    drop(project_guard);
    if label.is_some() {
        let _ = player::refresh(&state).await;
    }
    Ok(UndoOutcome { label, snapshot })
}

// ---------- asset ----------

#[tauri::command]
#[specta::specta]
pub async fn asset_import(
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> CmdResult<Vec<AssetId>> {
    let layout = {
        let guard = state.project.lock().await;
        guard.as_ref().ok_or_else(no_project)?.layout.clone()
    };
    // Decode nặng — không giữ lock project trong lúc import.
    let layout2 = layout.clone();
    let ids = tokio::task::spawn_blocking(move || assets_io::import_files(&layout2, &paths))
        .await
        .map_err(|e| IpcError::new(ErrorCode::Internal, format!("import task: {e}")))??;
    for id in &ids {
        let _ = state.handle().emit(
            "peaks:ready",
            serde_json::json!({ "asset_id": id.to_string() }),
        );
    }
    Ok(ids)
}

#[tauri::command]
#[specta::specta]
pub async fn asset_get(state: State<'_, AppState>, asset_id: String) -> CmdResult<AssetInfo> {
    let guard = state.project.lock().await;
    let project = guard.as_ref().ok_or_else(no_project)?;
    let asset = project
        .db
        .asset_get(&AssetId::from(asset_id))?
        .ok_or_else(|| IpcError::new(ErrorCode::AssetNotFound, "asset không tồn tại"))?;
    Ok(asset.into())
}

#[tauri::command]
#[specta::specta]
pub async fn asset_peaks(
    state: State<'_, AppState>,
    asset_id: String,
    zoom_level: u8,
) -> CmdResult<PeakView> {
    let layout = {
        let guard = state.project.lock().await;
        guard.as_ref().ok_or_else(no_project)?.layout.clone()
    };
    let id = AssetId::from(asset_id);
    let rel = AssetStore::rel_path(&id, "alspeak")
        .map_err(|e| IpcError::new(ErrorCode::AssetNotFound, e.to_string()))?;
    let store = AssetStore::new(layout.assets_dir())
        .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
    if !store.exists(&rel) {
        return Err(IpcError::new(
            ErrorCode::AssetNotFound,
            "peaks chưa sẵn sàng — chờ event peaks:ready",
        )
        .retryable(true));
    }
    let bytes = store
        .get(&rel)
        .map_err(|e| IpcError::new(ErrorCode::Io, e.to_string()))?;
    let mm = PeakMipmap::decode(&bytes)
        .map_err(|e| IpcError::new(ErrorCode::ProjectCorrupt, e.to_string()))?;
    let idx = (zoom_level as usize).min(mm.levels.len().saturating_sub(1));
    let level = &mm.levels[idx];
    Ok(PeakView {
        spp: level.spp,
        pairs: level.pairs.clone(),
    })
}

// ---------- generation ----------

#[tauri::command]
#[specta::specta]
pub async fn generate_submit(
    state: State<'_, AppState>,
    clip_id: String,
    recipe: GenerationRecipe,
    priority: Option<i32>,
) -> CmdResult<String> {
    let orch = state.orchestrator.lock().await.clone();
    let orch = orch.ok_or_else(no_project)?;
    let prio = priority.unwrap_or(job_priority::INTERACTIVE);
    let job = orch.submit_generate(clip_id, recipe, prio).await?;
    Ok(job.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn job_cancel(state: State<'_, AppState>, job_id: String) -> CmdResult<CancelOutcome> {
    let orch = state.orchestrator.lock().await.clone();
    let orch = orch.ok_or_else(no_project)?;
    Ok(orch.cancel(JobId::from(job_id)).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn take_list(state: State<'_, AppState>, clip_id: String) -> CmdResult<Vec<TakeInfo>> {
    let guard = state.project.lock().await;
    let project = guard.as_ref().ok_or_else(no_project)?;
    let takes = project.db.takes_for_clip(&clip_id)?;
    Ok(takes.into_iter().map(Into::into).collect())
}

#[tauri::command]
#[specta::specta]
pub async fn take_promote(
    state: State<'_, AppState>,
    clip_id: String,
    take_id: String,
) -> CmdResult<EditOutcome> {
    let out = apply_edit_inner(
        &state,
        EditCommand::SetActiveTake {
            clip_id: ClipId::from(clip_id),
            take_id: TakeId::from(take_id),
        },
    )
    .await?;
    let _ = player::refresh(&state).await;
    Ok(out)
}

#[tauri::command]
#[specta::specta]
pub async fn take_star(
    state: State<'_, AppState>,
    take_id: String,
    starred: bool,
) -> CmdResult<()> {
    let guard = state.project.lock().await;
    let project = guard.as_ref().ok_or_else(no_project)?;
    project.db.take_star(&TakeId::from(take_id), starred)?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn take_delete(state: State<'_, AppState>, take_id: String) -> CmdResult<()> {
    let guard = state.project.lock().await;
    let project = guard.as_ref().ok_or_else(no_project)?;
    project.db.take_delete(&TakeId::from(take_id))?;
    Ok(())
}

// ---------- transport ----------

async fn ensure_engine(state: &State<'_, AppState>) -> CmdResult<()> {
    let mut guard = state.engine.lock().await;
    if guard.is_none() {
        match als_audio::EngineBuilder::new(als_audio::AudioConfig::default()).start() {
            Ok(e) => *guard = Some(e),
            Err(e) => {
                return Err(IpcError::new(
                    ErrorCode::ProviderUnavailable,
                    format!("không mở được thiết bị audio: {e}"),
                )
                .retryable(true))
            }
        }
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transport_play(state: State<'_, AppState>) -> CmdResult<()> {
    ensure_engine(&state).await?;
    if let Some(e) = state.engine.lock().await.as_mut() {
        e.play();
    }
    state.playing.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transport_pause(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(e) = state.engine.lock().await.as_mut() {
        e.pause();
    }
    state.playing.store(false, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transport_seek(state: State<'_, AppState>, position_ms: u64) -> CmdResult<()> {
    ensure_engine(&state).await?;
    if let Some(e) = state.engine.lock().await.as_mut() {
        e.seek_ms(position_ms);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transport_loop(
    state: State<'_, AppState>,
    start_ms: u64,
    end_ms: u64,
    enabled: bool,
) -> CmdResult<()> {
    ensure_engine(&state).await?;
    if let Some(e) = state.engine.lock().await.as_mut() {
        e.set_loop(start_ms, end_ms, enabled);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn transport_position(state: State<'_, AppState>) -> CmdResult<TransportPosition> {
    let guard = state.engine.lock().await;
    let frames = guard.as_ref().map(|e| e.playhead().load_frames()).unwrap_or(0);
    Ok(TransportPosition {
        frames,
        playing: state.playing.load(Ordering::Relaxed),
    })
}

// ---------- engine ----------

#[tauri::command]
#[specta::specta]
pub async fn engine_status(state: State<'_, AppState>) -> CmdResult<EngineStatus> {
    let orch = state.orchestrator.lock().await.clone();
    match orch {
        Some(o) => Ok(o.engine_status().await?),
        // Chưa mở project → engine chưa sẵn sàng. TODO(S7): tách supervisor
        // engine ra khỏi session project để first-run tải model trước khi có project.
        None => Ok(EngineStatus {
            backend: ProviderId::from("none"),
            ready: false,
            warm_models: vec![],
            vram_free_mb: None,
            queue_depth: 0,
        }),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn engine_switch_backend(
    state: State<'_, AppState>,
    provider_id: String,
) -> CmdResult<()> {
    let orch = state.orchestrator.lock().await.clone();
    let orch = orch.ok_or_else(no_project)?;
    orch.switch_backend(ProviderId::from(provider_id)).await?;
    Ok(())
}

// ---------- export ----------

#[tauri::command]
#[specta::specta]
pub async fn export_render(state: State<'_, AppState>, spec: ExportSpec) -> CmdResult<String> {
    let (tracks, layout) = {
        let guard = state.project.lock().await;
        let p = guard.as_ref().ok_or_else(no_project)?;
        (p.manifest.arrangement.tracks.clone(), p.layout.clone())
    };
    let spec2 = spec.clone();
    // Bounce nặng CPU/IO — không giữ lock, không chặn runtime.
    tokio::task::spawn_blocking(move || {
        player::bounce(&layout, &tracks, &spec2, env!("CARGO_PKG_VERSION"))
    })
    .await
    .map_err(|e| IpcError::new(ErrorCode::Internal, format!("export task: {e}")))?
}
