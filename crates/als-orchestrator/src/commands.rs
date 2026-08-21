use crate::error::Result;
use als_core::{EngineStatus, GenerationRecipe, JobId, ProviderId};
use als_provider::CancelOutcome;
use tokio::sync::oneshot;

/// Lệnh gửi vào orchestrator task. Mỗi lệnh có oneshot trả lời — caller ở
/// src-tauri await lấy kết quả đồng bộ theo nghĩa IPC.
pub enum OrchCommand {
    SubmitGenerate {
        clip_id: String,
        recipe: Box<GenerationRecipe>,
        priority: i32,
        resp: oneshot::Sender<Result<JobId>>,
    },
    Cancel {
        job_id: JobId,
        resp: oneshot::Sender<Result<CancelOutcome>>,
    },
    EngineStatus {
        resp: oneshot::Sender<EngineStatus>,
    },
    SwitchBackend {
        provider: ProviderId,
        resp: oneshot::Sender<Result<()>>,
    },
    Shutdown,
}
