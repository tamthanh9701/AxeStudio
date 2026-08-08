use als_core::{ErrorCode, IpcError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider không sẵn sàng: {0}")]
    Unavailable(String),
    #[error("capability không được hỗ trợ: {0:?}")]
    CapabilityNotSupported(crate::types::Capability),
    #[error("model chưa cài: {0}")]
    ModelMissing(String),
    #[error("worker trả lỗi: {0}")]
    Worker(String),
    #[error("lỗi mạng/HTTP: {0}")]
    Http(String),
    #[error("job đã bị huỷ")]
    Cancelled,
    #[error("hết thời gian chờ")]
    Timeout,
    #[error("dữ liệu worker trả về không hợp lệ: {0}")]
    InvalidResponse(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ProviderError>;

impl From<ProviderError> for IpcError {
    fn from(e: ProviderError) -> Self {
        match e {
            ProviderError::Unavailable(m) => {
                IpcError::new(ErrorCode::ProviderUnavailable, m).retryable(true)
            }
            ProviderError::CapabilityNotSupported(c) => IpcError::new(
                ErrorCode::CapabilityNotSupported,
                format!("capability {c:?} không có trên provider hiện tại"),
            ),
            ProviderError::ModelMissing(m) => IpcError::new(ErrorCode::ModelMissing, m),
            ProviderError::Worker(m) => IpcError::new(ErrorCode::Internal, m).retryable(true),
            ProviderError::Http(m) => {
                IpcError::new(ErrorCode::ProviderUnavailable, m).retryable(true)
            }
            ProviderError::Cancelled => IpcError::new(ErrorCode::JobNotFound, "job đã bị huỷ"),
            ProviderError::Timeout => {
                IpcError::new(ErrorCode::ProviderUnavailable, "timeout").retryable(true)
            }
            ProviderError::InvalidResponse(m) => IpcError::new(ErrorCode::Internal, m),
            ProviderError::Io(e) => IpcError::new(ErrorCode::Io, e.to_string()),
        }
    }
}
