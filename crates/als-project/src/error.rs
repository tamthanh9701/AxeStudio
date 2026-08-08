use als_core::{ErrorCode, IpcError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("project hỏng: {0}")]
    Corrupt(String),
    #[error("schema project ({found}) mới hơn app hỗ trợ ({supported}) — cập nhật app")]
    SchemaTooNew { found: u32, supported: u32 },
    #[error("project đã tồn tại tại: {0}")]
    AlreadyExists(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ProjectError>;

impl From<ProjectError> for IpcError {
    fn from(e: ProjectError) -> Self {
        match e {
            ProjectError::SchemaTooNew { found, supported } => IpcError::new(
                ErrorCode::SchemaTooNew,
                format!("project schema {found}, app hỗ trợ {supported}"),
            ),
            ProjectError::Corrupt(m) | ProjectError::AlreadyExists(m) => {
                IpcError::new(ErrorCode::ProjectCorrupt, m)
            }
            ProjectError::Json(e) => IpcError::new(ErrorCode::ProjectCorrupt, e.to_string()),
            ProjectError::Io(e) => IpcError::new(ErrorCode::Io, e.to_string()),
            ProjectError::Sqlite(e) => IpcError::new(ErrorCode::Internal, e.to_string()),
        }
    }
}
