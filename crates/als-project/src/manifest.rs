//! manifest.json — document của project. Ghi ATOMIC: temp → fsync → rename.
//! Kill process giữa lúc save không được phép làm hỏng file (acceptance S3).

use crate::error::{ProjectError, Result};
use crate::migrations::SCHEMA_VERSION;
use als_core::{Arrangement, ProjectId};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub project_id: ProjectId,
    pub name: String,
    pub created_at_unix: i64,
    /// Version app đã ghi — phục vụ diagnostics.
    pub app_version: String,
    pub arrangement: Arrangement,
}

impl Manifest {
    pub fn new(name: impl Into<String>, app_version: impl Into<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            project_id: ProjectId::new(),
            name: name.into(),
            created_at_unix: crate::db::now_unix(),
            app_version: app_version.into(),
            arrangement: Arrangement::default(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let m: Manifest = serde_json::from_slice(&bytes)?;
        if m.schema_version > SCHEMA_VERSION {
            return Err(ProjectError::SchemaTooNew {
                found: m.schema_version,
                supported: SCHEMA_VERSION,
            });
        }
        Ok(m)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self)?;
        write_atomic(path, &bytes)?;
        Ok(())
    }
}

/// Ghi file theo kiểu atomic: ghi bản đầy đủ ra file tạm CÙNG THƯ MỤC,
/// fsync, rồi rename (rename là nguyên tử trên cả NTFS lẫn POSIX).
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    // fsync thư mục cha để chắc rename bền qua crash — chỉ có nghĩa trên Unix.
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_save_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        let m = Manifest::new("test song", "0.0.1");
        m.save(&path).unwrap();
        let loaded = Manifest::load(&path).unwrap();
        assert_eq!(loaded.name, "test song");
        assert_eq!(loaded.schema_version, SCHEMA_VERSION);
        // Không được sót file .tmp sau khi save.
        assert!(!dir.path().join("manifest.tmp").exists());
    }

    #[test]
    fn rejects_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        let mut m = Manifest::new("x", "0.0.1");
        m.schema_version = SCHEMA_VERSION + 1;
        m.save(&path).unwrap();
        let err = Manifest::load(&path).unwrap_err();
        assert!(matches!(err, ProjectError::SchemaTooNew { .. }));
    }
}
