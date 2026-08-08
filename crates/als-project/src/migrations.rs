//! Migration một chiều lên (ADR-004). CẤM sửa migration đã release — thêm file mới.

use crate::error::{ProjectError, Result};
use rusqlite::Connection;

/// Version schema hiện hành. Tăng khi thêm migration; bằng số migration mới nhất.
pub const SCHEMA_VERSION: u32 = 1;

pub(crate) const MIGRATIONS: &[(u32, &str)] = &[(1, include_str!("../migrations/001_init.sql"))];

pub(crate) fn current_version(conn: &Connection) -> Result<u32> {
    let exists: bool = conn.query_row(
        "SELECT count(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'schema_version'",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(0);
    }
    let v: u32 = conn.query_row("SELECT version FROM schema_version LIMIT 1", [], |r| r.get(0))?;
    Ok(v)
}

/// Chạy mọi migration có version > current, trong MỘT transaction.
pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    let current = current_version(conn)?;
    if current > SCHEMA_VERSION {
        return Err(ProjectError::SchemaTooNew {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    if current == SCHEMA_VERSION {
        return Ok(());
    }
    for (v, sql) in MIGRATIONS {
        if *v > current {
            conn.execute_batch(sql)?;
        }
    }
    if current == 0 {
        conn.execute("INSERT INTO schema_version (version) VALUES (?1)", [SCHEMA_VERSION])?;
    } else {
        conn.execute("UPDATE schema_version SET version = ?1", [SCHEMA_VERSION])?;
    }
    Ok(())
}
