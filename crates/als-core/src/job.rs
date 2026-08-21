//! Kiểu job + engine status + export — đi qua IPC.

use crate::id::ProviderId;
use serde::{Deserialize, Serialize};

/// Priority: số càng lớn càng được nhặt trước (idx_job_pick trong migration 001).
pub mod priority {
    /// Preview ngắn khi người dùng đang chờ trực tiếp.
    pub const PREVIEW: i32 = 300;
    /// Generate tương tác thường.
    pub const INTERACTIVE: i32 = 200;
    /// Batch / export nền.
    pub const BATCH: i32 = 100;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Plan,
    Render,
    Understand,
    Extract,
    Lego,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Dispatching,
    Running,
    Postprocess,
    Done,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobState::Done | JobState::Failed | JobState::Cancelled
        )
    }
}

/// Payload của `engine_status` và event `engine:status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct EngineStatus {
    pub backend: ProviderId,
    pub ready: bool,
    pub warm_models: Vec<String>,
    /// specta: u64 bị cấm export — MB luôn < 2^53.
    #[specta(type = Option<i32>)]
    pub vram_free_mb: Option<u64>,
    pub queue_depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    /// Master WAV 48kHz 24-bit.
    Wav24,
    Mp3,
    Flac,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExportRange {
    Project,
    Loop {
        #[specta(type = i32)]
        start_ms: u64,
        #[specta(type = i32)]
        end_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ExportSpec {
    pub format: ExportFormat,
    pub range: ExportRange,
    pub out_path: String,
    /// Nhúng metadata recipe + disclosure "AI-generated" (plan §S6).
    pub include_metadata: bool,
}
