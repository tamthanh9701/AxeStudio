//! als-assets — content-addressed store + peaks mipmap.
//!
//! Asset được định danh bằng blake3 của NỘI DUNG: cùng bytes → cùng id →
//! import hai lần cùng một file không tốn thêm dung lượng (dedupe tự nhiên).

#![forbid(unsafe_code)]

pub mod peaks;
pub mod store;

pub use peaks::{PeakLevel, PeakMipmap, PeaksError, LEVEL_SPACING};
pub use store::{AssetStore, StoreError};
