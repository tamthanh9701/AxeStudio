-- Migration 001 — DDL đúng contract plan §6.1.
-- PRAGMA (journal_mode, synchronous, foreign_keys) là connection-level,
-- được set trong db.rs mỗi lần mở, KHÔNG nằm ở đây.

CREATE TABLE schema_version (version INTEGER NOT NULL);

CREATE TABLE asset (
  id           TEXT PRIMARY KEY,   -- blake3 hex, content-addressed
  kind         TEXT NOT NULL,      -- import | render | stem | peaks
  rel_path     TEXT NOT NULL,
  bytes        INTEGER NOT NULL,
  sample_rate  INTEGER,
  channels     INTEGER,
  duration_ms  INTEGER,
  created_at   INTEGER NOT NULL
);

-- Cache TẦNG 1: kết quả LM. Đổi seed/steps KHÔNG làm mất hiệu lực bản ghi này.
CREATE TABLE plan_cache (
  plan_hash    TEXT PRIMARY KEY,
  provider_id  TEXT NOT NULL,
  model_id     TEXT NOT NULL,
  audio_codes  TEXT NOT NULL,      -- FSQ tokens
  lyrics       TEXT,
  metas_json   TEXT NOT NULL,
  hits         INTEGER NOT NULL DEFAULT 0,
  created_at   INTEGER NOT NULL
);

-- Cache TẦNG 2: kết quả DiT.
CREATE TABLE take (
  id           TEXT PRIMARY KEY,
  clip_id      TEXT NOT NULL,
  recipe_json  TEXT NOT NULL,
  plan_hash    TEXT NOT NULL,
  render_hash  TEXT NOT NULL,
  asset_id     TEXT NOT NULL REFERENCES asset(id),
  lufs         REAL,
  true_peak_db REAL,
  starred      INTEGER NOT NULL DEFAULT 0,
  created_at   INTEGER NOT NULL
);
CREATE UNIQUE INDEX idx_take_render ON take(render_hash);
CREATE INDEX        idx_take_clip   ON take(clip_id, created_at DESC);

CREATE TABLE job (
  id           TEXT PRIMARY KEY,
  kind         TEXT NOT NULL,      -- plan | render | understand | extract | lego
  state        TEXT NOT NULL,      -- queued|dispatching|running|postprocess|done|failed|cancelled
  priority     INTEGER NOT NULL,   -- 300 preview > 200 interactive > 100 batch
  payload_json TEXT NOT NULL,
  provider_id  TEXT,
  external_id  TEXT,               -- task_id phía worker
  error        TEXT,
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL
);
CREATE INDEX idx_job_pick ON job(state, priority DESC, created_at);
