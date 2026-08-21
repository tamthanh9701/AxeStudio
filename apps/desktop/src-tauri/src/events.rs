//! Payload các event Rust → UI, định nghĩa MỘT LẦN và đăng ký vào
//! tauri-specta để `pnpm bindings:generate` xuất ra `generated.ts`.
//!
//! Tên event khớp docs/contracts/ipc.md ("job:state", ...) — vì vậy trait
//! `tauri_specta::Event` được impl tay thay vì derive (derive sinh tên từ
//! tên struct, không chứa dấu hai chấm được).

use als_core::{AssetId, JobId, JobState, TakeId};
use als_provider::ProgressStage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct JobStateEvent {
    pub job_id: JobId,
    pub state: JobState,
    pub error: Option<String>,
}

impl tauri_specta::Event for JobStateEvent {
    const NAME: &'static str = "job:state";
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct JobProgressEvent {
    pub job_id: JobId,
    /// 0..=100
    pub percent: u8,
    pub stage: ProgressStage,
}

impl tauri_specta::Event for JobProgressEvent {
    const NAME: &'static str = "job:progress";
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct TakeReadyEvent {
    pub job_id: JobId,
    pub clip_id: String,
    pub take_id: TakeId,
    /// true = cache tầng 2 trúng — take đã tồn tại, UI gắn bằng SetActiveTake.
    pub cached: bool,
}

impl tauri_specta::Event for TakeReadyEvent {
    const NAME: &'static str = "take:ready";
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct PeaksReadyEvent {
    pub asset_id: AssetId,
}

impl tauri_specta::Event for PeaksReadyEvent {
    const NAME: &'static str = "peaks:ready";
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct ProjectDirtyEvent {
    pub dirty: bool,
}

impl tauri_specta::Event for ProjectDirtyEvent {
    const NAME: &'static str = "project:dirty";
}
