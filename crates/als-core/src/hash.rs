//! Cache 2 tầng (ADR-003).
//!
//! - `plan_hash` — kết quả pha LM. Đổi seed/steps KHÔNG làm mất hiệu lực.
//! - `render_hash` — kết quả pha DiT. = f(plan_hash, sampling, overrides,
//!   model_checksum, PIPELINE_VERSION).
//!
//! ĐỂ Ý: cả hai hash đều trộn thêm `provider_id` + `model_id` so với contract
//! gốc trong plan (plan_cache PK = plan_hash đơn lẻ). Lý do: nếu không, cùng
//! một recipe chạy qua hai provider sẽ đọc nhầm audio_codes của nhau (mock
//! provider có thể đầu độc cache của provider thật). Cần ADR-005 ghi nhận.

use crate::canonicalize::{canonicalize, CanonicalizeError};
use crate::id::ProviderId;
use crate::recipe::GenerationRecipe;
use serde::{Deserialize, Serialize};

/// Version của toàn bộ pipeline hậu kỳ. BẤT KỲ thay đổi nào ở canonicalize,
/// normalize, hoặc postprocess audio đều phải TĂNG số này.
pub const PIPELINE_VERSION: u32 = 1;

/// blake3 hex của canonical(recipe \ sampling) + provider + model.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlanHash(pub String);

/// blake3 hex của plan_hash + sampling + overrides + model_checksum + PIPELINE_VERSION.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RenderHash(pub String);

impl std::fmt::Display for PlanHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::fmt::Display for RenderHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HashError {
    #[error(transparent)]
    Canonicalize(#[from] CanonicalizeError),
}

/// Tầng 1 — kết quả LM. Đổi seed/steps/guidance KHÔNG làm mất hiệu lực.
pub fn plan_hash(
    recipe: &GenerationRecipe,
    provider: &ProviderId,
    model_id: &str,
) -> Result<PlanHash, HashError> {
    let canonical = canonicalize(&recipe.plan_view())?;
    let mut h = blake3::Hasher::new();
    h.update(canonical.as_bytes());
    h.update(b"\x00");
    h.update(provider.0.as_bytes());
    h.update(b"\x00");
    h.update(model_id.as_bytes());
    Ok(PlanHash(h.finalize().to_hex().to_string()))
}

/// Tầng 2 — kết quả DiT, dùng PIPELINE_VERSION hiện hành.
pub fn render_hash(
    recipe: &GenerationRecipe,
    provider: &ProviderId,
    model_id: &str,
    model_checksum: &str,
) -> Result<RenderHash, HashError> {
    render_hash_with_version(recipe, provider, model_id, model_checksum, PIPELINE_VERSION)
}

/// Tách version thành tham số để test được "đổi PIPELINE_VERSION → hash đổi".
pub fn render_hash_with_version(
    recipe: &GenerationRecipe,
    provider: &ProviderId,
    model_id: &str,
    model_checksum: &str,
    pipeline_version: u32,
) -> Result<RenderHash, HashError> {
    let plan = plan_hash(recipe, provider, model_id)?;
    let sampling_canonical = canonicalize(&recipe.sampling_view())?;
    let mut h = blake3::Hasher::new();
    h.update(plan.0.as_bytes());
    h.update(b"\x00");
    h.update(sampling_canonical.as_bytes());
    h.update(b"\x00");
    h.update(model_checksum.as_bytes());
    h.update(b"\x00");
    h.update(pipeline_version.to_le_bytes());
    Ok(RenderHash(h.finalize().to_hex().to_string()))
}
