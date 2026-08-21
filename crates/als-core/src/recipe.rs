//! GenerationRecipe — đầu vào của một lần sinh nhạc.
//!
//! Recipe phải TRUNG LẬP với provider: không chứa trường riêng của ACE-Step
//! (lm_backend, shift...). Tham số riêng nằm ở `provider_overrides`, không
//! tham gia plan_hash nhưng có tham gia render_hash (ADR-003).

use crate::canonicalize::{normalize_block, normalize_line};
use crate::error::{ErrorCode, IpcError};
use crate::id::AssetId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Task của ACE-Step. Serialize khớp đúng chuỗi `task_type` của engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub enum TaskType {
    #[serde(rename = "text2music")]
    Text2Music,
    #[serde(rename = "cover")]
    Cover,
    #[serde(rename = "repaint")]
    Repaint,
    #[serde(rename = "lego")]
    Lego,
    #[serde(rename = "extract")]
    Extract,
    #[serde(rename = "complete")]
    Complete,
}

/// Tier model trừu tượng. Mỗi provider map tier → model id cụ thể
/// (vd `turbo` → `acestep-v15-turbo` hoặc `acestep-v15-turbo-Q8_0.gguf`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub enum ModelTier {
    #[serde(rename = "turbo")]
    Turbo,
    #[serde(rename = "sft")]
    Sft,
    #[serde(rename = "base")]
    Base,
    #[serde(rename = "xl_turbo")]
    XlTurbo,
    #[serde(rename = "xl_sft")]
    XlSft,
    #[serde(rename = "xl_base")]
    XlBase,
}

impl ModelTier {
    /// Turbo dùng 8 steps; các tier còn lại mặc định 50.
    pub fn default_steps(self) -> u32 {
        match self {
            ModelTier::Turbo | ModelTier::XlTurbo => 8,
            _ => 50,
        }
    }

    /// extract/lego/complete chỉ tồn tại trên base (xem Model Zoo trong plan).
    pub fn supports_task(self, task: TaskType) -> bool {
        match task {
            TaskType::Extract | TaskType::Lego | TaskType::Complete => {
                matches!(self, ModelTier::Base | ModelTier::XlBase)
            }
            TaskType::Text2Music | TaskType::Cover => true,
            // repaint chạy được trên cả turbo lẫn sft, nhưng khuyến nghị sft.
            TaskType::Repaint => !matches!(self, ModelTier::Base | ModelTier::XlBase),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum InferMethod {
    Ode,
    Sde,
}

/// Tham số sampling — CHỈ nhóm này tham gia render_hash, không tham gia plan_hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SamplingParams {
    /// None = engine tự random (`use_random_seed=true` phía ACE-Step).
    /// specta: u64 bị cấm export — seed chỉ hiển thị ở UI, `number` đủ
    /// (giá trị > 2^53 sẽ hiển thị xấp xỉ; engine vẫn nhận đủ 64 bit).
    #[specta(type = Option<f64>)]
    pub seed: Option<u64>,
    pub inference_steps: u32,
    /// Chỉ có tác dụng trên tier base. UI phải ẩn khi model là turbo.
    pub guidance_scale: Option<f32>,
    /// Chỉ có tác dụng trên tier base.
    pub shift: Option<f32>,
    pub infer_method: InferMethod,
    /// 1..=8
    pub batch_size: u32,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            seed: None,
            inference_steps: 8,
            guidance_scale: None,
            shift: None,
            infer_method: InferMethod::Ode,
            batch_size: 2,
        }
    }
}

/// Tham số riêng của provider/engine. Không tham gia plan_hash,
/// CÓ tham gia render_hash. `extra` là BTreeMap — luôn duyệt theo thứ tự sort.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, specta::Type)]
pub struct ProviderOverrides {
    /// `vllm` | `pt` — chỉ có nghĩa với provider py.
    pub lm_backend: Option<String>,
    #[serde(default)]
    /// specta: serde_json::Value liệt kê i64/u64 nội bộ bị cấm export —
    /// UI chỉ đọc qua, export thành `Record<string, unknown>`.
    #[specta(type = std::collections::BTreeMap<String, specta_typescript::Unknown>)]
    pub extra: BTreeMap<String, serde_json::Value>,
    pub lm_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct GenerationRecipe {
    /// Caption / style tags. Chuỗi rỗng + lyrics rỗng = engine tự sinh cả hai.
    pub prompt: String,
    /// `""` = LM tự sinh lời; `"[Instrumental]"` = không vocal (chuỗi engine đã train).
    pub lyrics: String,
    /// 10..=600 giây.
    pub duration_s: u32,
    /// 30..=300
    pub bpm: Option<u32>,
    pub key_scale: Option<String>,
    /// Chỉ nhận 2 | 3 | 4 | 6.
    pub time_signature: Option<u8>,
    /// Mã ngôn ngữ vocal, vd `"vi"`, `"en"`. None = để LM tự suy.
    pub vocal_language: Option<String>,
    pub task: TaskType,
    pub model_tier: ModelTier,
    /// Audio tham chiếu phong cách (cover/complete).
    pub reference_audio: Option<AssetId>,
    /// Audio nguồn (repaint/cover/extract/lego).
    pub source_audio: Option<AssetId>,
    /// (start_ms, end_ms) — bắt buộc khi task = repaint.
    /// specta: u64 bị cấm export — ms luôn < 2^53 nên `number` là đủ.
    #[specta(type = Option<(i32, i32)>)]
    pub repaint_range_ms: Option<(u64, u64)>,
    pub sampling: SamplingParams,
    #[serde(default)]
    pub provider_overrides: ProviderOverrides,
}

impl GenerationRecipe {
    /// Kiểm tra ràng buộc giá trị. Gọi ở BIÊN IPC (trong `generate_submit`),
    /// trước khi job vào queue — không tin dữ liệu từ UI.
    pub fn validate(&self) -> Result<(), IpcError> {
        let bad = |msg: &str| IpcError::new(ErrorCode::InvalidRecipe, msg);

        if !(10..=600).contains(&self.duration_s) {
            return Err(bad("duration_s phải trong 10..=600"));
        }
        if let Some(bpm) = self.bpm {
            if !(30..=300).contains(&bpm) {
                return Err(bad("bpm phải trong 30..=300"));
            }
        }
        if let Some(ts) = self.time_signature {
            if !matches!(ts, 2 | 3 | 4 | 6) {
                return Err(bad("time_signature chỉ nhận 2 | 3 | 4 | 6"));
            }
        }
        if !(1..=8).contains(&self.sampling.batch_size) {
            return Err(bad("batch_size phải trong 1..=8"));
        }
        if self.sampling.inference_steps == 0 {
            return Err(bad("inference_steps phải >= 1"));
        }
        let max_steps = match self.model_tier {
            ModelTier::Turbo | ModelTier::XlTurbo => 20,
            _ => 200,
        };
        if self.sampling.inference_steps > max_steps {
            return Err(bad("inference_steps vượt giới hạn của tier model"));
        }
        if !self.model_tier.supports_task(self.task) {
            return Err(bad("model tier không hỗ trợ task này"));
        }
        match self.task {
            TaskType::Repaint => {
                if self.source_audio.is_none() || self.repaint_range_ms.is_none() {
                    return Err(bad("repaint cần source_audio và repaint_range_ms"));
                }
                if let Some((start, end)) = self.repaint_range_ms {
                    if start >= end {
                        return Err(bad("repaint_range_ms: start phải < end"));
                    }
                }
            }
            TaskType::Cover | TaskType::Extract | TaskType::Lego => {
                if self.source_audio.is_none() {
                    return Err(bad("task này cần source_audio"));
                }
            }
            TaskType::Complete => {
                if self.reference_audio.is_none() {
                    return Err(bad("complete cần reference_audio"));
                }
            }
            TaskType::Text2Music => {}
        }
        Ok(())
    }

    /// View phục vụ plan_hash: recipe TRỪ sampling và provider_overrides.
    /// Chuỗi được chuẩn hoá theo field (prompt = 1 dòng, lyrics = khối dòng).
    pub(crate) fn plan_view(&self) -> serde_json::Value {
        serde_json::json!({
            "prompt": normalize_line(&self.prompt),
            "lyrics": normalize_block(&self.lyrics),
            "duration_s": self.duration_s,
            "bpm": self.bpm,
            "key_scale": self.key_scale.as_deref().map(normalize_line),
            "time_signature": self.time_signature,
            "vocal_language": self.vocal_language,
            "task": self.task,
            "model_tier": self.model_tier,
            "reference_audio": self.reference_audio,
            "source_audio": self.source_audio,
            "repaint_range_ms": self.repaint_range_ms,
        })
    }

    /// View phục vụ render_hash: sampling + provider_overrides.
    pub(crate) fn sampling_view(&self) -> serde_json::Value {
        serde_json::json!({
            "sampling": self.sampling,
            "provider_overrides": self.provider_overrides,
        })
    }
}
