use std::path::{Path, PathBuf};

/// Vị trí các thành phần trong một `.aiproj`.
#[derive(Debug, Clone)]
pub struct ProjectLayout {
    root: PathBuf,
}

impl ProjectLayout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root.join("manifest.json")
    }

    pub fn db_path(&self) -> PathBuf {
        self.root.join("project.db")
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub fn create_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.assets_dir())?;
        std::fs::create_dir_all(self.tmp_dir())?;
        Ok(())
    }

    pub fn exists(&self) -> bool {
        self.manifest_path().exists()
    }
}
