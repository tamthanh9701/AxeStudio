//! Kiểu biên provider dùng chung — sống ở als-core vì chúng xuất hiện trong
//! IPC payload (`EngineStatus`) mà als-core không được phụ thuộc als-provider
//! (AGENTS.md §2). `als_provider` re-export lại để chỗ cũ không đổi import.
//!
//! UI đọc danh sách này để ẩn tính năng — **CẤM** hardcode trong component
//! (issue #10 / ALS-F05).

use crate::{ModelTier, TaskType};
use serde::{Deserialize, Serialize};

/// Định danh model cụ thể phía provider (vd `acestep-v15-turbo`,
/// `acestep-v15-turbo-Q8_0.gguf`). Không trùng với [`crate::ModelTier`] —
/// tier là trừu tượng, id là hiện thực.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
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

impl From<String> for ModelId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for ModelId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Những gì provider làm được.
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

/// Một model provider nhận diện được (đã cài / biết đường tải).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ModelDescriptor {
    pub id: ModelId,
    pub tier: ModelTier,
    /// Checksum file trọng số (blake3 hex). Đi vào render_hash — đổi quant
    /// (Q8_0 → Q4_K_M) đổi checksum → không trả nhầm cache cũ.
    pub checksum: String,
    /// specta: u64 bị cấm export — MB luôn < 2^53.
    #[specta(type = Option<i32>)]
    pub vram_estimate_mb: Option<u64>,
    pub warm: bool,
}
