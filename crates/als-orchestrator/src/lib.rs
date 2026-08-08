//! als-orchestrator — job queue bền + cache 2 tầng + dispatch + postprocess.
//!
//! Actor pattern: rusqlite::Connection là Send-nhưng-không-Sync, nên toàn bộ
//! Db sống trong MỘT task duy nhất. Bên ngoài nói chuyện qua [`OrchCommand`]
//! (mpsc) và nhận [`OrchEvent`] (broadcast). Không có Mutex nào ở đây.
//!
//! Luồng generate (ADR-003):
//! 1. validate recipe (biên IPC — không tin UI)
//! 2. tra `render_hash` → trúng: xong ngay, không gọi worker
//! 3. tra `plan_hash` → trúng: bỏ qua pha LM
//! 4. render → postprocess (loudness, peaks, asset) → take row

#![forbid(unsafe_code)]

mod commands;
mod error;
mod events;
mod orchestrator;
mod registry;

pub use commands::OrchCommand;
pub use error::{OrchError, Result};
pub use events::OrchEvent;
pub use orchestrator::{spawn, OrchestratorHandle};
pub use registry::Registry;
