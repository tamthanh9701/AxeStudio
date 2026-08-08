use crate::db::Db;
use crate::error::Result;
use crate::layout::ProjectLayout;
use crate::manifest::Manifest;
use als_core::{Arrangement, ProjectSnapshot, PROJECT_SCHEMA_VERSION};
use std::path::Path;

/// Project đang mở: manifest (document) + db (index) + layout (đường dẫn).
pub struct Project {
    pub layout: ProjectLayout,
    pub db: Db,
    pub manifest: Manifest,
}

impl Project {
    /// Tạo project mới. Fail nếu đã có project tại path (contract project_create).
    pub fn create(root: &Path, name: &str, app_version: &str) -> Result<Self> {
        let layout = ProjectLayout::new(root);
        if layout.exists() {
            return Err(crate::error::ProjectError::AlreadyExists(
                root.display().to_string(),
            ));
        }
        layout.create_dirs()?;
        let manifest = Manifest::new(name, app_version);
        manifest.save(&layout.manifest_path())?;
        let db = Db::open(&layout.db_path())?;
        // Crash recovery: mọi job kẹt từ phiên trước (không thể có khi create,
        // nhưng giữ đối xứng với open).
        db.job_fail_interrupted()?;
        Ok(Self {
            layout,
            db,
            manifest,
        })
    }

    /// Mở project: load manifest, migrate db nếu cũ hơn, từ chối nếu mới hơn.
    pub fn open(root: &Path) -> Result<Self> {
        let layout = ProjectLayout::new(root);
        let manifest = Manifest::load(&layout.manifest_path())?;
        let db = Db::open(&layout.db_path())?;
        db.job_fail_interrupted()?;
        Ok(Self {
            layout,
            db,
            manifest,
        })
    }

    /// Lưu document. Index (db) tự bền theo từng thao tác; chỉ manifest cần save.
    pub fn save(&mut self) -> Result<()> {
        self.manifest.save(&self.layout.manifest_path())
    }

    pub fn replace_arrangement(&mut self, arrangement: Arrangement) -> Result<()> {
        self.manifest.arrangement = arrangement;
        self.save()
    }

    pub fn snapshot(&self, dirty: bool) -> ProjectSnapshot {
        ProjectSnapshot {
            project_id: self.manifest.project_id.clone(),
            name: self.manifest.name.clone(),
            schema_version: PROJECT_SCHEMA_VERSION,
            arrangement: self.manifest.arrangement.clone(),
            dirty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_open_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("song.aiproj");
        {
            let p = Project::create(&root, "song one", "0.0.1").unwrap();
            assert!(p.layout.manifest_path().exists());
            assert!(p.layout.db_path().exists());
        }
        let p2 = Project::open(&root).unwrap();
        let snap = p2.snapshot(false);
        assert_eq!(snap.name, "song one");
        assert_eq!(snap.schema_version, PROJECT_SCHEMA_VERSION);
    }

    #[test]
    fn create_refuses_existing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("song.aiproj");
        Project::create(&root, "a", "0.0.1").unwrap();
        let err = Project::create(&root, "b", "0.0.1").unwrap_err();
        assert!(matches!(err, crate::error::ProjectError::AlreadyExists(_)));
    }
}
