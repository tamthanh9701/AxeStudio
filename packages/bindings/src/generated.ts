// ============================================================================
// AUTO-GENERATED — CẤM SỬA TAY.
//
// Sinh bởi: pnpm bindings:generate  (cargo run -p als-desktop --bin export-bindings)
// CI job `bindings-drift` fail nếu file này lệch với Rust types ở als-core.
//
// BẢN PLACEHOLDER: viết tay khớp serde shape để frontend develop được ngay.
// Lần chạy `bindings:generate` đầu tiên sẽ ghi đè file này.
// ============================================================================

// ---------- id newtypes ----------
export type JobId = string
export type ClipId = string
export type TakeId = string
export type TrackId = string
export type ProjectId = string
export type AssetId = string
export type ProviderId = string
export type ModelId = string

// ---------- error (SCREAMING_SNAKE_CASE — khớp serde rename_all) ----------
export type ErrorCode =
  | "PROJECT_NOT_FOUND"
  | "PROJECT_CORRUPT"
  | "SCHEMA_TOO_NEW"
  | "ASSET_NOT_FOUND"
  | "ASSET_IN_USE"
  | "PROVIDER_UNAVAILABLE"
  | "CAPABILITY_NOT_SUPPORTED"
  | "JOB_NOT_FOUND"
  | "JOB_TOO_LATE"
  | "MODEL_MISSING"
  | "VRAM_EXHAUSTED"
  | "INVALID_RECIPE"
  | "EXPORT_FAILED"
  | "IO"
  | "INTERNAL"

export type IpcError = { code: ErrorCode; message: string; retryable: boolean }

// ---------- recipe ----------
export type TaskType = "text2music" | "cover" | "repaint" | "lego" | "extract" | "complete"
export type ModelTier = "turbo" | "sft" | "base" | "xl_turbo" | "xl_sft" | "xl_base"
export type InferMethod = "ode" | "sde"

export type SamplingParams = {
  seed: number | null
  inference_steps: number
  guidance_scale: number | null
  shift: number | null
  infer_method: InferMethod
  batch_size: number
}

export type ProviderOverrides = {
  lm_backend: string | null
  lm_model: string | null
  extra: Record<string, unknown>
}

export type GenerationRecipe = {
  prompt: string
  lyrics: string
  duration_s: number
  bpm: number | null
  key_scale: string | null
  time_signature: number | null
  vocal_language: string | null
  task: TaskType
  model_tier: ModelTier
  reference_audio: AssetId | null
  source_audio: AssetId | null
  repaint_range_ms: [number, number] | null
  sampling: SamplingParams
  provider_overrides: ProviderOverrides
}

// ---------- document ----------
export type TrackKind = "audio" | "generated"
export type ClipSource = { type: "imported"; asset: AssetId } | { type: "generated" }
export type GenerationInfo = { recipe: GenerationRecipe }

export type Clip = {
  id: ClipId
  start_ms: number
  duration_ms: number
  offset_ms: number
  gain_db: number
  fade_in_ms: number
  fade_out_ms: number
  source: ClipSource
  generation: GenerationInfo | null
  active_take: TakeId | null
}

export type Track = {
  id: TrackId
  kind: TrackKind
  name: string
  gain_db: number
  pan: number
  mute: boolean
  solo: boolean
  clips: Clip[]
}

export type Arrangement = { tracks: Track[] }

export type ProjectSnapshot = {
  project_id: ProjectId
  name: string
  schema_version: number
  arrangement: Arrangement
  dirty: boolean
}

export type TakeInfo = {
  id: TakeId
  clip_id: string
  plan_hash: string
  render_hash: string
  asset_id: AssetId
  lufs: number | null
  true_peak_db: number | null
  starred: boolean
  created_at_unix: number
}

// ---------- edit ----------
export type EditCommand =
  | { op: "add_track"; kind: TrackKind; name: string }
  | { op: "remove_track"; track_id: TrackId }
  | {
      op: "add_clip"
      track_id: TrackId
      clip_id: ClipId
      start_ms: number
      duration_ms: number
      source: ClipSource
    }
  | { op: "move_clip"; clip_id: ClipId; to_track: TrackId; start_ms: number }
  | { op: "trim_clip"; clip_id: ClipId; start_ms: number; duration_ms: number; offset_ms: number }
  | { op: "split_clip"; clip_id: ClipId; at_ms: number }
  | { op: "set_track_gain"; track_id: TrackId; gain_db: number }
  | { op: "set_track_pan"; track_id: TrackId; pan: number }
  | { op: "set_track_mute"; track_id: TrackId; mute: boolean }
  | { op: "set_track_solo"; track_id: TrackId; solo: boolean }
  | { op: "set_active_take"; clip_id: ClipId; take_id: TakeId }
  | { op: "remove_clip"; clip_id: ClipId }

export type EditResult = { label: string }

/// project_apply_edit trả cả snapshot mới — UI không bao giờ được lệch state.
export type EditOutcome = { edit: EditResult; snapshot: ProjectSnapshot }

// ---------- job / engine ----------
export type JobKind = "plan" | "render" | "understand" | "extract" | "lego" | "export"
export type JobState =
  | "queued"
  | "dispatching"
  | "running"
  | "postprocess"
  | "done"
  | "failed"
  | "cancelled"

export type EngineStatus = {
  backend: ProviderId
  ready: boolean
  warm_models: string[]
  vram_free_mb: number | null
  queue_depth: number
}

export type ExportFormat = "wav24" | "mp3" | "flac"
export type ExportRange = { type: "project" } | { type: "loop"; start_ms: number; end_ms: number }
export type ExportSpec = {
  format: ExportFormat
  range: ExportRange
  out_path: string
  include_metadata: boolean
}

// ---------- provider ----------
export type Capability =
  | "text2music"
  | "cover"
  | "repaint"
  | "lego"
  | "extract"
  | "complete"
  | "understand"
  | "hot_swap_model"
  | "cancel_running_job"
  | "lora_training"
  | "split_plan_render"

export type ProgressStage = "queued" | "planning" | "rendering" | "postprocess"
export type CancelOutcome = "cancelled" | "too_late"

export type Health = {
  ready: boolean
  vram_free_mb: number | null
  loaded_models: ModelId[]
  detail: string | null
}

export type ModelDescriptor = {
  id: ModelId
  tier: ModelTier
  checksum: string
  vram_estimate_mb: number | null
  warm: boolean
}

// ---------- event payloads (Rust → UI) ----------
export type JobStateEvent = { job_id: JobId; state: JobState; error: string | null }
export type JobProgressEvent = { job_id: JobId; percent: number; stage: ProgressStage }
export type TakeReadyEvent = { job_id: JobId; clip_id: string; take_id: TakeId; cached: boolean }
export type PeaksReadyEvent = { asset_id: AssetId }
export type ProjectDirtyEvent = { dirty: boolean }

// ---------- misc views ----------
export type TransportPosition = { frames: number; playing: boolean }
export type PeakView = { spp: number; pairs: [number, number][] }
