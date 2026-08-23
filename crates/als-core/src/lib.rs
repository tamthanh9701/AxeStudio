//! als-core — kiểu chung, id, lỗi, canonicalize + hash, document model, kiểu IPC.
//!
//! Crate này là nguồn sự thật cho contract. Mọi thay đổi public API ở đây
//! phải kèm regenerate bindings (`pnpm bindings:generate`) trong cùng PR.
//!
//! Xem `docs/contracts/ipc.md` và `docs/adr/ADR-003-two-tier-cache.md`.

#![forbid(unsafe_code)]

pub mod canonicalize;
pub mod document;
pub mod edit;
pub mod error;
pub mod hash;
pub mod id;
pub mod job;
pub mod provider_types;
pub mod recipe;

pub use canonicalize::{canonicalize, normalize_block, normalize_line, CanonicalizeError};
pub use document::{
    Arrangement, Clip, ClipSource, EditCommand, EditResult, GenerationInfo, ProjectSnapshot,
    TakeInfo, Track, TrackKind, PROJECT_SCHEMA_VERSION,
};
pub use edit::UndoStack;
pub use error::{ErrorCode, IpcError};
pub use hash::{
    plan_hash, render_hash, render_hash_with_version, HashError, PlanHash, RenderHash,
    PIPELINE_VERSION,
};
pub use id::{AssetId, ClipId, JobId, ProjectId, ProviderId, TakeId, TrackId};
pub use job::{priority, EngineStatus, ExportFormat, ExportRange, ExportSpec, JobKind, JobState};
pub use provider_types::{Capability, ModelDescriptor, ModelId};
pub use recipe::{
    GenerationRecipe, InferMethod, ModelTier, ProviderOverrides, SamplingParams, TaskType,
};
