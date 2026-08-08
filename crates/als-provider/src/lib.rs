//! als-provider — trait RenderProvider + contract tests + MockProvider.
//!
//! Đây là ĐIỂM CHẠM DUY NHẤT giữa orchestrator và các engine sinh nhạc.
//! Orchestrator không được biết đang nói chuyện với provider nào (ADR-001).
//!
//! Mọi provider mới phải chạy bộ test ở [`contract`] — không viết bộ test riêng.

#![forbid(unsafe_code)]

pub mod contract;
pub mod error;
pub mod mock;
pub mod provider;
pub mod types;

pub use error::{ProviderError, Result};
pub use mock::MockProvider;
pub use provider::{JobCtx, RenderProvider};
pub use types::{
    AudioAnalysis, AudioBlob, AudioFormat, CancelOutcome, Capability, Health, ModelDescriptor,
    ModelId, PlanInput, PlanOutput, Progress, ProgressStage, RenderInput, RenderOutput, Slot,
    UnderstandInput,
};
