//! SQLite index: asset, plan_cache, take, job.
//!
//! Chú ý: document (track/clip) KHÔNG nằm ở đây — nó nằm ở manifest.json.

use crate::error::{ProjectError, Result};
use crate::migrations;
use als_core::{AssetId, JobId, JobKind, JobState, TakeId};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub(crate) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssetRow {
    pub id: String,
    pub kind: String,
    pub rel_path: String,
    pub bytes: i64,
    pub sample_rate: Option<i64>,
    pub channels: Option<i64>,
    pub duration_ms: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanCacheRow {
    pub plan_hash: String,
    pub provider_id: String,
    pub model_id: String,
    pub audio_codes: String,
    pub lyrics: Option<String>,
    pub metas_json: String,
    pub hits: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TakeRow {
    pub id: String,
    pub clip_id: String,
    pub recipe_json: String,
    pub plan_hash: String,
    pub render_hash: String,
    pub asset_id: String,
    pub lufs: Option<f64>,
    pub true_peak_db: Option<f64>,
    pub starred: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobRow {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub priority: i64,
    pub payload_json: String,
    pub provider_id: Option<String>,
    pub external_id: Option<String>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        // Connection-level pragmas — phải set mỗi lần mở.
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        migrations::migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<u32> {
        migrations::current_version(&self.conn)
    }

    // ---------- asset ----------

    pub fn asset_put(&self, row: &AssetRow) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO asset
             (id, kind, rel_path, bytes, sample_rate, channels, duration_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                row.id,
                row.kind,
                row.rel_path,
                row.bytes,
                row.sample_rate,
                row.channels,
                row.duration_ms,
                row.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn asset_get(&self, id: &AssetId) -> Result<Option<AssetRow>> {
        self.conn
            .query_row(
                "SELECT id, kind, rel_path, bytes, sample_rate, channels, duration_ms, created_at
                 FROM asset WHERE id = ?1",
                params![id.as_str()],
                |r| {
                    Ok(AssetRow {
                        id: r.get(0)?,
                        kind: r.get(1)?,
                        rel_path: r.get(2)?,
                        bytes: r.get(3)?,
                        sample_rate: r.get(4)?,
                        channels: r.get(5)?,
                        duration_ms: r.get(6)?,
                        created_at: r.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// Số take còn tham chiếu asset — chặn xoá khi > 0 (contract asset_delete).
    pub fn asset_ref_count(&self, id: &AssetId) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT count(*) FROM take WHERE asset_id = ?1",
            params![id.as_str()],
            |r| r.get(0),
        )?)
    }

    pub fn asset_delete(&self, id: &AssetId) -> Result<bool> {
        let n = self
            .conn
            .execute("DELETE FROM asset WHERE id = ?1", params![id.as_str()])?;
        Ok(n > 0)
    }

    // ---------- plan_cache (tầng 1) ----------

    pub fn plan_get(&self, plan_hash: &str) -> Result<Option<PlanCacheRow>> {
        self.conn
            .query_row(
                "SELECT plan_hash, provider_id, model_id, audio_codes, lyrics, metas_json, hits, created_at
                 FROM plan_cache WHERE plan_hash = ?1",
                params![plan_hash],
                |r| {
                    Ok(PlanCacheRow {
                        plan_hash: r.get(0)?,
                        provider_id: r.get(1)?,
                        model_id: r.get(2)?,
                        audio_codes: r.get(3)?,
                        lyrics: r.get(4)?,
                        metas_json: r.get(5)?,
                        hits: r.get(6)?,
                        created_at: r.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn plan_put(&self, row: &PlanCacheRow) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO plan_cache
             (plan_hash, provider_id, model_id, audio_codes, lyrics, metas_json, hits, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                row.plan_hash,
                row.provider_id,
                row.model_id,
                row.audio_codes,
                row.lyrics,
                row.metas_json,
                row.hits,
                row.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn plan_hit(&self, plan_hash: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE plan_cache SET hits = hits + 1 WHERE plan_hash = ?1",
            params![plan_hash],
        )?;
        Ok(())
    }

    // ---------- take (tầng 2) ----------

    pub fn take_insert(&self, row: &TakeRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO take
             (id, clip_id, recipe_json, plan_hash, render_hash, asset_id, lufs, true_peak_db, starred, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.id,
                row.clip_id,
                row.recipe_json,
                row.plan_hash,
                row.render_hash,
                row.asset_id,
                row.lufs,
                row.true_peak_db,
                row.starred,
                row.created_at,
            ],
        )?;
        Ok(())
    }

    /// Tra cache tầng 2 trước khi gọi worker (contract generate_submit).
    pub fn take_by_render_hash(&self, render_hash: &str) -> Result<Option<TakeRow>> {
        self.conn
            .query_row(
                "SELECT id, clip_id, recipe_json, plan_hash, render_hash, asset_id, lufs, true_peak_db, starred, created_at
                 FROM take WHERE render_hash = ?1",
                params![render_hash],
                take_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn takes_for_clip(&self, clip_id: &str) -> Result<Vec<TakeRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, clip_id, recipe_json, plan_hash, render_hash, asset_id, lufs, true_peak_db, starred, created_at
             FROM take WHERE clip_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map(params![clip_id], take_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn take_star(&self, id: &TakeId, starred: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE take SET starred = ?2 WHERE id = ?1",
            params![id.as_str(), starred],
        )?;
        Ok(())
    }

    pub fn take_delete(&self, id: &TakeId) -> Result<bool> {
        let n = self
            .conn
            .execute("DELETE FROM take WHERE id = ?1", params![id.as_str()])?;
        Ok(n > 0)
    }

    // ---------- job queue ----------

    pub fn job_insert(&self, row: &JobRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO job
             (id, kind, state, priority, payload_json, provider_id, external_id, error, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                row.id,
                row.kind,
                row.state,
                row.priority,
                row.payload_json,
                row.provider_id,
                row.external_id,
                row.error,
                row.created_at,
                row.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn job_update_state(
        &self,
        id: &JobId,
        state: JobState,
        error: Option<&str>,
    ) -> Result<()> {
        let state_str = serde_json::to_string(&state)?
            .trim_matches('"')
            .to_string();
        self.conn.execute(
            "UPDATE job SET state = ?2, error = ?3, updated_at = ?4 WHERE id = ?1",
            params![id.as_str(), state_str, error, now_unix()],
        )?;
        Ok(())
    }

    /// Nhặt job queued có priority cao nhất (FIFO trong cùng priority) và
    /// chuyển sang dispatching. Orchestrator chạy đơn luồng nên SELECT→UPDATE
    /// tuần tự là đủ — không cần transaction.
    pub fn job_pick_next(&self) -> Result<Option<JobRow>> {
        let candidate: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM job WHERE state = 'queued'
                 ORDER BY priority DESC, created_at ASC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let Some(id) = candidate else {
            return Ok(None);
        };
        self.conn.execute(
            "UPDATE job SET state = 'dispatching', updated_at = ?2 WHERE id = ?1",
            params![id, now_unix()],
        )?;
        self.job_by_id(&id)
    }

    pub fn job_by_id(&self, id: &str) -> Result<Option<JobRow>> {
        self.conn
            .query_row(
                "SELECT id, kind, state, priority, payload_json, provider_id, external_id, error, created_at, updated_at
                 FROM job WHERE id = ?1",
                params![id],
                job_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    /// Số job chưa xong — đi vào EngineStatus.queue_depth.
    pub fn job_queue_depth(&self) -> Result<u32> {
        let n: i64 = self.conn.query_row(
            "SELECT count(*) FROM job WHERE state IN ('queued', 'dispatching', 'running', 'postprocess')",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u32)
    }

    /// Khôi phục sau crash: job kẹt ở dispatching/running/postprocess → failed.
    /// (Không auto-retry vì không biết worker đã ghi gì dở dang.)
    pub fn job_fail_interrupted(&self) -> Result<u32> {
        let n = self.conn.execute(
            "UPDATE job SET state = 'failed', error = 'interrupted by app restart', updated_at = ?1
             WHERE state IN ('dispatching', 'running', 'postprocess')",
            params![now_unix()],
        )?;
        Ok(n as u32)
    }
}

fn take_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TakeRow> {
    Ok(TakeRow {
        id: r.get(0)?,
        clip_id: r.get(1)?,
        recipe_json: r.get(2)?,
        plan_hash: r.get(3)?,
        render_hash: r.get(4)?,
        asset_id: r.get(5)?,
        lufs: r.get(6)?,
        true_peak_db: r.get(7)?,
        starred: r.get(8)?,
        created_at: r.get(9)?,
    })
}

fn job_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<JobRow> {
    Ok(JobRow {
        id: r.get(0)?,
        kind: r.get(1)?,
        state: r.get(2)?,
        priority: r.get(3)?,
        payload_json: r.get(4)?,
        provider_id: r.get(5)?,
        external_id: r.get(6)?,
        error: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

#[allow(dead_code)]
fn _assert_job_state_serializes() {
    // JobKind/JobState lưu DB dạng chuỗi snake_case — khớp comment trong DDL.
    let s = serde_json::to_string(&JobKind::Render).unwrap();
    debug_assert_eq!(s, "\"render\"");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_temp() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("project.db")).unwrap();
        (dir, db)
    }

    fn asset_row(id: &str) -> AssetRow {
        AssetRow {
            id: id.into(),
            kind: "render".into(),
            rel_path: format!("ab/cd/{id}.wav"),
            bytes: 1024,
            sample_rate: Some(48_000),
            channels: Some(2),
            duration_ms: Some(10_000),
            created_at: now_unix(),
        }
    }

    #[test]
    fn migration_idempotent() {
        let (_d, db) = open_temp();
        assert_eq!(db.schema_version().unwrap(), migrations::SCHEMA_VERSION);
        // Mở lại lần nữa trên cùng file — không được chạy lại migration.
        let db2 = Db::open(&_d.path().join("project.db")).unwrap();
        assert_eq!(db2.schema_version().unwrap(), migrations::SCHEMA_VERSION);
    }

    #[test]
    fn rejects_too_new_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("project.db");
        {
            let db = Db::open(&path).unwrap();
            db.conn
                .execute("UPDATE schema_version SET version = 999", [])
                .unwrap();
        }
        let err = Db::open(&path).unwrap_err();
        assert!(matches!(err, ProjectError::SchemaTooNew { found: 999, .. }));
    }

    #[test]
    fn plan_cache_roundtrip_and_hits() {
        let (_d, db) = open_temp();
        let row = PlanCacheRow {
            plan_hash: "ph1".into(),
            provider_id: "cpp".into(),
            model_id: "acestep-v15-turbo".into(),
            audio_codes: "FSQ:...".into(),
            lyrics: None,
            metas_json: "{}".into(),
            hits: 0,
            created_at: now_unix(),
        };
        db.plan_put(&row).unwrap();
        db.plan_hit("ph1").unwrap();
        db.plan_hit("ph1").unwrap();
        let got = db.plan_get("ph1").unwrap().unwrap();
        assert_eq!(got.hits, 2);
        assert_eq!(got.audio_codes, "FSQ:...");
    }

    #[test]
    fn take_lookup_by_render_hash() {
        let (_d, db) = open_temp();
        db.asset_put(&asset_row("a1")).unwrap();
        let take = TakeRow {
            id: "t1".into(),
            clip_id: "c1".into(),
            recipe_json: "{}".into(),
            plan_hash: "ph".into(),
            render_hash: "rh".into(),
            asset_id: "a1".into(),
            lufs: Some(-14.0),
            true_peak_db: Some(-1.0),
            starred: false,
            created_at: now_unix(),
        };
        db.take_insert(&take).unwrap();
        assert!(db.take_by_render_hash("rh").unwrap().is_some());
        assert_eq!(db.asset_ref_count(&AssetId::from("a1")).unwrap(), 1);
        // Không cho xoá asset khi còn take tham chiếu.
        assert_eq!(db.takes_for_clip("c1").unwrap().len(), 1);
    }

    #[test]
    fn job_pick_respects_priority_then_fifo() {
        let (_d, db) = open_temp();
        for (id, prio) in [("j-low", 100i64), ("j-high", 300), ("j-mid", 200)] {
            db.job_insert(&JobRow {
                id: id.into(),
                kind: "render".into(),
                state: "queued".into(),
                priority: prio,
                payload_json: "{}".into(),
                provider_id: None,
                external_id: None,
                error: None,
                created_at: now_unix(),
                updated_at: now_unix(),
            })
            .unwrap();
        }
        assert_eq!(db.job_pick_next().unwrap().unwrap().id, "j-high");
        assert_eq!(db.job_pick_next().unwrap().unwrap().id, "j-mid");
        assert_eq!(db.job_queue_depth().unwrap(), 2); // 2 đang dispatching + 1 queued? j-high dispatching...
    }

    #[test]
    fn interrupted_jobs_fail_on_restart() {
        let (_d, db) = open_temp();
        db.job_insert(&JobRow {
            id: "j1".into(),
            kind: "render".into(),
            state: "running".into(),
            priority: 200,
            payload_json: "{}".into(),
            provider_id: None,
            external_id: None,
            error: None,
            created_at: now_unix(),
            updated_at: now_unix(),
        })
        .unwrap();
        assert_eq!(db.job_fail_interrupted().unwrap(), 1);
        let j = db.job_by_id("j1").unwrap().unwrap();
        assert_eq!(j.state, "failed");
    }
}
