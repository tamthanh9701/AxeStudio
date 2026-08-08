//! Content-addressed store.
//!
//! Layout trên đĩa (sharding 2 tầng để tránh thư mục triệu file):
//! ```txt
//! assets/
//!   ab/           — 2 ký tự đầu của blake3 hex
//!     cd/         — 2 ký tự tiếp theo
//!       abcd1234....wav
//! ```

use als_core::AssetId;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("asset id không hợp lệ: {0}")]
    BadId(String),
}

pub struct AssetStore {
    root: PathBuf,
}

impl AssetStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Đường dẫn tương đối từ id + extension (không có dấu chấm).
    pub fn rel_path(id: &AssetId, ext: &str) -> Result<String, StoreError> {
        let hex = id.as_str();
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(StoreError::BadId(hex.to_owned()));
        }
        Ok(format!("{}/{}/{}.{}", &hex[0..2], &hex[2..4], hex, ext))
    }

    pub fn abs_path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// Ghi bytes vào store, trả (AssetId, rel_path). Idempotent: file đã có
    /// thì bỏ qua ghi, trả thẳng id — đây chính là cơ chế dedupe.
    pub fn put(&self, bytes: &[u8], ext: &str) -> Result<(AssetId, String), StoreError> {
        let id = AssetId::from_content_hash(&blake3::hash(bytes));
        let rel = Self::rel_path(&id, ext)?;
        let abs = self.abs_path(&rel);
        if abs.exists() {
            return Ok((id, rel));
        }
        if let Some(dir) = abs.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Ghi atomic: temp cùng thư mục → rename. Kill giữa chừng không sót file dở.
        let tmp = abs.with_extension("part");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &abs)?;
        Ok((id, rel))
    }

    pub fn get(&self, rel: &str) -> Result<Vec<u8>, StoreError> {
        Ok(std::fs::read(self.abs_path(rel))?)
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.abs_path(rel).exists()
    }

    pub fn delete(&self, rel: &str) -> Result<bool, StoreError> {
        let abs = self.abs_path(rel);
        if abs.exists() {
            std::fs::remove_file(abs)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_is_idempotent_and_dedupes() {
        let dir = tempfile::tempdir().unwrap();
        let store = AssetStore::new(dir.path()).unwrap();
        let (id1, rel1) = store.put(b"hello audio", "wav").unwrap();
        let (id2, rel2) = store.put(b"hello audio", "wav").unwrap();
        assert_eq!(id1, id2);
        assert_eq!(rel1, rel2);
        // 2 lần put → chỉ 1 file trên đĩa.
        let count = walkdir_count(dir.path());
        assert_eq!(count, 1);
    }

    #[test]
    fn different_content_different_id() {
        let dir = tempfile::tempdir().unwrap();
        let store = AssetStore::new(dir.path()).unwrap();
        let (a, _) = store.put(b"a", "wav").unwrap();
        let (b, _) = store.put(b"b", "wav").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn sharding_shape() {
        let dir = tempfile::tempdir().unwrap();
        let store = AssetStore::new(dir.path()).unwrap();
        let (id, rel) = store.put(b"x", "wav").unwrap();
        let hex = id.as_str();
        assert_eq!(rel, format!("{}/{}/{}.wav", &hex[0..2], &hex[2..4], hex));
        assert!(store.exists(&rel));
        assert_eq!(store.get(&rel).unwrap(), b"x");
    }

    #[test]
    fn rejects_bad_id() {
        assert!(AssetStore::rel_path(&AssetId::from("zzz"), "wav").is_err());
        assert!(AssetStore::rel_path(&AssetId::from("short"), "wav").is_err());
    }

    fn walkdir_count(root: &Path) -> usize {
        fn go(d: &Path, n: &mut usize) {
            for e in std::fs::read_dir(d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    go(&p, n);
                } else {
                    *n += 1;
                }
            }
        }
        let mut n = 0;
        go(root, &mut n);
        n
    }
}
