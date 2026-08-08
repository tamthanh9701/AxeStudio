//! als-core — kiểu chung, id, lỗi, canonicalize + hash.
//!
//! Crate này là nguồn sự thật cho contract. Mọi thay đổi public API ở đây
//! phải kèm regenerate bindings (`pnpm bindings:generate`) trong cùng PR.
//!
//! Xem `docs/contracts/ipc.md` và `docs/adr/ADR-003-two-tier-cache.md`.

#![forbid(unsafe_code)]

pub mod canonicalize;
pub mod error;
pub mod hash;
pub mod id;
pub mod recipe;

pub use canonicalize::{canonicalize, normalize_block, normalize_line, CanonicalizeError};
pub use error::{ErrorCode, IpcError};
pub use hash::{plan_hash, render_hash, render_hash_with_version, HashError, PlanHash, RenderHash, PIPELINE_VERSION};
pub use id::{AssetId, ClipId, JobId, ProjectId, ProviderId, TakeId, TrackId};
pub use recipe::{
    GenerationRecipe, InferMethod, ModelTier, ProviderOverrides, SamplingParams, TaskType,
};
