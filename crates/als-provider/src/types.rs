//! Kiểu dữ liệu đi qua biên provider.
//!
//! Kiểu nào xuất hiện trong IPC (engine_status, events) thì derive specta::Type.
//! Kiểu chỉ sống trong Rust (RenderOutput chứa bytes) thì không cần.

use als_core::{AssetId, GenerationRecipe, JobId, ModelTier, ProviderId, TaskType};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Định danh model cụ thể phía provider (vd `acestep-v15-turbo`,
/// `acestep-v15-turbo-Q8_0.gguf`). Không trùng với ModelTier — tier là trừu tượng,
/// id là hiện thực.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct ModelId(pub String);

impl std::fmt::Display for ModelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ModelId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// Những gì provider làm được. UI đọc danh sách này để ẩn tính năng —
/// CẤM hardcode trong component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Text2Music,
    Cover,
    Repaint,
    Lego,
    Extract,
    Complete,
    Understand,
    HotSwapModel,
    CancelRunningJob,
    LoraTraining,
    /// Provider tách được pha LM (plan) và pha DiT (render) thành 2 lời gọi.
    /// cpp có (`/lm` + `/synth`); py KHÔNG — release_task là single-shot,
    /// dù nó chấp nhận `audio_code_string` để bỏ qua LM ở lần sau.
    SplitPlanRender,
}

impl Capability {
    pub fn for_task(task: TaskType) -> Self {
        match task {
            TaskType::Text2Music => Capability::Text2Music,
            TaskType::Cover => Capability::Cover,
            TaskType::Repaint => Capability::Repaint,
            TaskType::Lego => Capability::Lego,
            TaskType::Extract => Capability::Extract,
            TaskType::Complete => Capability::Complete,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Health {
    pub ready: bool,
    pub vram_free_mb: Option<u64>,
    pub loaded_models: Vec<ModelId>,
    /// Chi tiết cho trang Diagnostics — không hiển thị ở UI chính.
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ModelDescriptor {
    pub id: ModelId,
    pub tier: ModelTier,
    /// Checksum file trọng số (blake3 hex). Đi vào render_hash — đổi quant
    /// (Q8_0 → Q4_K_M) đổi checksum → không trả nhầm cache cũ.
    pub checksum: String,
    pub vram_estimate_mb: Option<u64>,
    pub warm: bool,
}

/// Slot nạp model nóng. ACE-Step `/v1/init` nhận slot 1..=3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct Slot(pub u8);

impl Slot {
    pub fn new(n: u8) -> Result<Self, ()> {
        if (1..=3).contains(&n) {
            Ok(Self(n))
        } else {
            Err(())
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlanInput {
    /// Provider chỉ được đọc phần phi-sampling của recipe.
    pub recipe: GenerationRecipe,
    pub model: ModelId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanOutput {
    /// FSQ tokens — `audio_code_string` của ACE-Step.
    /// Rỗng với provider non-split (orchestrator truyền plan rỗng vào render()).
    pub audio_codes: String,
    /// Lyrics sau khi LM xử lý (LM tự sinh nếu input rỗng).
    pub lyrics: Option<String>,
    /// bpm/key/duration mà LM suy ra — giữ nguyên dạng JSON.
    pub metas: serde_json::Value,
}

impl PlanOutput {
    /// Plan rỗng cho provider non-split: render() tự lo cả hai pha.
    pub fn opaque() -> Self {
        Self {
            audio_codes: String::new(),
            lyrics: None,
            metas: serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderInput {
    pub recipe: GenerationRecipe,
    /// Từ `plan()` hoặc từ plan_cache — orchestrator quyết định, provider không care.
    pub plan: PlanOutput,
    pub model: ModelId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    /// PCM float32 LE, 48kHz stereo — output chuẩn của cả hai backend.
    WavF32,
}

#[derive(Debug, Clone)]
pub struct AudioBlob {
    pub format: AudioFormat,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct RenderOutput {
    pub audio: AudioBlob,
    /// Seed thực tế đã dùng (engine random khi recipe không chỉ định).
    pub seed_used: u64,
    pub duration_ms: u64,
    /// audio_codes engine đã dùng/sinh ra — Some khi provider trả được
    /// (py trả trong result json; cpp đã có từ plan()). Orchestrator dùng để
    /// backfill plan_cache, để re-roll seed lần sau bỏ qua được pha LM.
    pub audio_codes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UnderstandInput {
    /// Orchestrator resolve AssetId → path thật trước khi gọi.
    pub audio_path: PathBuf,
    pub question: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioAnalysis {
    pub caption: String,
    pub bpm: Option<f32>,
    pub key_scale: Option<String>,
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ProgressStage {
    Queued,
    Planning,
    Rendering,
    Postprocess,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Progress {
    pub job_id: JobId,
    /// 0..=100
    pub percent: u8,
    pub stage: ProgressStage,
}

/// Ba kết cục cancel — KHÔNG được giả vờ huỷ thành công (AGENTS.md §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CancelOutcome {
    Cancelled,
    /// Job đã dispatch xuống worker, không huỷ được nữa. UI phải nói rõ điều này.
    TooLate,
}

/// ProviderId re-export ở đây cho tiện import phía provider crates.
pub type Provider = ProviderId;
/// AssetId re-export tương tự.
pub type Asset = AssetId;
