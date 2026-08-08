//! Trait RenderProvider — contract giữa orchestrator và engine (plan §6.2).

use crate::error::Result;
use crate::types::{
    AudioAnalysis, CancelOutcome, Capability, Health, ModelDescriptor, ModelId, PlanInput,
    PlanOutput, Progress, RenderInput, RenderOutput, Slot, UnderstandInput,
};
use als_core::{JobId, ProviderId};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Ngữ cảnh đi kèm mọi lời gọi dài: huỷ + báo tiến độ.
pub struct JobCtx {
    pub job_id: JobId,
    pub cancel: CancellationToken,
    pub progress: mpsc::Sender<Progress>,
}

impl JobCtx {
    /// Tiện cho provider: báo tiến độ, bỏ qua nếu receiver đã đóng.
    pub async fn report(&self, percent: u8, stage: crate::types::ProgressStage) {
        let _ = self
            .progress
            .send(Progress {
                job_id: self.job_id.clone(),
                percent: percent.min(100),
                stage,
            })
            .await;
    }
}

#[async_trait]
pub trait RenderProvider: Send + Sync {
    fn id(&self) -> ProviderId;

    /// Danh sách capability — ổn định trong suốt vòng đời provider.
    fn capabilities(&self) -> &[Capability];

    /// Sức khoẻ hiện tại. Phải trả lời nhanh (< 1s), không được block.
    async fn health(&self) -> Result<Health>;

    /// Model provider nhận diện được (đã cài / biết đường tải).
    async fn models(&self) -> Result<Vec<ModelDescriptor>>;

    /// Pha 1 — chỉ LM. Orchestrator cache kết quả theo plan_hash.
    async fn plan(&self, input: PlanInput, cx: JobCtx) -> Result<PlanOutput>;

    /// Pha 2 — chỉ DiT + VAE. Orchestrator cache theo render_hash.
    async fn render(&self, input: RenderInput, cx: JobCtx) -> Result<RenderOutput>;

    async fn understand(&self, input: UnderstandInput, cx: JobCtx) -> Result<AudioAnalysis>;

    /// Nạp nóng một model vào slot. No-op nếu đã warm.
    async fn warmup(&self, model: &ModelId, slot: Slot) -> Result<()>;

    /// Huỷ job. Trả `TooLate` khi job đã dispatch — CẤM giả vờ Cancelled.
    async fn cancel(&self, job: &JobId) -> Result<CancelOutcome>;
}
