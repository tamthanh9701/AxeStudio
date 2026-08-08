//! Lỗi đi qua IPC. `code` là enum ĐÓNG — UI map code sang thông báo tiếng Việt,
//! không hiển thị `message` thô cho người dùng cuối.
//!
//! Thêm variant mới = thay đổi contract → regenerate bindings trong cùng PR.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    ProjectNotFound,
    ProjectCorrupt,
    /// Project được tạo bởi app version mới hơn — từ chối mở, yêu cầu cập nhật.
    SchemaTooNew,
    AssetNotFound,
    /// Xoá asset khi vẫn còn take/clip tham chiếu.
    AssetInUse,
    ProviderUnavailable,
    /// Provider hiện tại không có capability mà task yêu cầu (vd cpp không có repaint).
    CapabilityNotSupported,
    JobNotFound,
    /// Huỷ job khi đã dispatch xuống worker — xem ADR-001 mục Hệ quả.
    JobTooLate,
    ModelMissing,
    VramExhausted,
    InvalidRecipe,
    ExportFailed,
    Io,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct IpcError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl IpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
        }
    }

    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for IpcError {}
