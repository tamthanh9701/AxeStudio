//! als-provider-py — client cho `acestep-api` (Python, port 8001).
//!
//! Khác biệt bản chất so với cpp: API này SINGLE-SHOT. Một `POST /release_task`
//! chạy cả LM lẫn DiT; không có endpoint plan riêng. Provider này vì thế
//! KHÔNG khai báo `Capability::SplitPlanRender` — orchestrator sẽ gọi render()
//! one-shot, rồi backfill plan_cache từ `audio_codes` trong response.
//!
//! API không có cancel, không có WebSocket/SSE → poll `POST /query_result`.
//! Response wrapper chung: `{ data, code, error, timestamp, extra }`.
//!
//! Field name theo docs/en/API.md của ACE-Step 1.5; điểm chưa xác nhận đánh
//! dấu `TODO(S-02)` — spike Phase 0 phải kiểm chứng trên server thật.

#![forbid(unsafe_code)]

mod client;
mod payload;
mod provider;

pub use client::AcestepApiClient;
pub use payload::{build_release_payload, model_id_for_tier, AssetResolver};
pub use provider::PyProvider;
