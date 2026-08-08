use als_core::{ErrorCode, IpcError};
use als_assets::StoreError;
use als_media::MediaError;
use als_project::ProjectError;
use als_provider::ProviderError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrchError {
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Media(#[from] MediaError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("recipe không hợp lệ: {0}")]
    InvalidRecipe(#[from] IpcError),
    #[error("job không tồn tại hoặc đã kết thúc: {0}")]
    JobNotFound(String),
    #[error("provider không có trong registry: {0}")]
    NoProvider(String),
}

pub type Result<T> = std::result::Result<T, OrchError>;

impl From<OrchError> for IpcError {
    fn from(e: OrchError) -> Self {
        match e {
            OrchError::InvalidRecipe(ie) => ie,
            OrchError::Provider(pe) => pe.into(),
            OrchError::Project(pe) => pe.into(),
            OrchError::JobNotFound(m) => IpcError::new(ErrorCode::JobNotFound, m),
            OrchError::NoProvider(m) => IpcError::new(ErrorCode::ProviderUnavailable, m),
            OrchError::Store(se) => IpcError::new(ErrorCode::Io, se.to_string()),
            OrchError::Media(me) => IpcError::new(ErrorCode::Internal, me.to_string()),
            OrchError::Json(je) => IpcError::new(ErrorCode::Internal, je.to_string()),
        }
    }
}
