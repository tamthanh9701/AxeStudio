//! als-project — project store: manifest.json (document) + SQLite (index).
//!
//! Layout `.aiproj`:
//! ```txt
//! my-song.aiproj/
//! ├── manifest.json   — document: track/clip, đọc được bằng mắt
//! ├── project.db      — SQLite WAL: asset, take, job, plan_cache
//! ├── assets/         — content-addressed (als-assets quản lý)
//! └── tmp/
//! ```

#![forbid(unsafe_code)]

pub mod db;
pub mod error;
pub mod layout;
pub mod manifest;
pub mod migrations;
pub mod project;

pub use db::{AssetRow, Db, JobRow, PlanCacheRow, TakeRow};
pub use error::{ProjectError, Result};
pub use layout::ProjectLayout;
pub use manifest::Manifest;
pub use migrations::SCHEMA_VERSION;
pub use project::Project;
