import { useState } from "react"
import type { GenerationRecipe, ModelTier, TaskType } from "@als/bindings"
import { explainError, ipc } from "../../ipc/client"
import { useStudio } from "../../state/store"
import {
  availableTasks,
  availableTiers,
  lyricsHint,
  recipeProblems,
  visibleControls,
} from "./rules"

export function defaultRecipe(): GenerationRecipe {
  return {
    prompt: "",
    lyrics: "",
    duration_s: 30,
    bpm: null,
    key_scale: null,
    time_signature: 4,
    vocal_language: null,
    task: "text2music",
    model_tier: "turbo",
    reference_audio: null,
    source_audio: null,
    repaint_range_ms: null,
    sampling: {
      seed: null,
      inference_steps: 8,
      guidance_scale: null,
      shift: null,
      infer_method: "ode",
      batch_size: 1,
    },
    provider_overrides: { lm_backend: null, lm_model: null, extra: {} },
  }
}

/**
 * LABEL duy nhất — KHÔNG phải nguồn sự thật về tính khả dụng (đó là
 * engine_status.capabilities/models, issue #10).
 */
const TASK_LABEL: Record<TaskType, string> = {
  text2music: "Text → Music",
  cover: "Cover",
  repaint: "Repaint (sửa một vùng)",
  extract: "Extract stem",
  lego: "Lego (thêm track)",
  complete: "Complete",
}

const TIER_LABEL: Record<ModelTier, string> = {
  turbo: "Turbo — nhanh (8 steps)",
  sft: "SFT — chất lượng cao (50 steps)",
  base: "Base — extract/lego/complete",
  xl_turbo: "XL Turbo — nhanh (8 steps)",
  xl_sft: "XL SFT (50 steps)",
  xl_base: "XL Base — extract/lego/complete",
}

function isActive(state: string): boolean {
  return state !== "done" && state !== "failed" && state !== "cancelled"
}

function JobList() {
  const jobs = useStudio((s) => s.jobs)
  const active = Object.entries(jobs).filter(([, j]) => isActive(j.state))
  if (active.length === 0) return null
  return (
    <div className="jobs">
      {active.map(([id, j]) => {
        const warm = id.startsWith("warm:")
        return (
          <div key={id} className="job-item">
            <div className="job-bar">
              <div className="job-bar-fill" style={{ width: `${j.percent}%` }} />
            </div>
            <span className="dim job-stage">
              {warm ? "Nạp model" : j.stage} · {j.percent}%
            </span>
            {!warm && <button onClick={() => void ipc.jobCancel(id).catch(() => {})}>Huỷ</button>}
          </div>
        )
      })}
    </div>
  )
}

export function GeneratePanel() {
  const [recipe, setRecipe] = useState<GenerationRecipe>(defaultRecipe)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<string | null>(null)
  const snapshot = useStudio((s) => s.snapshot)
  const engine = useStudio((s) => s.engine)
  // ALS-F05 (#10): tính khả dụng đọc TỪ engine_status — CẤM hardcode.
  const availTasks = engine ? availableTasks(engine.capabilities) : []
  const availTiers = engine ? availableTiers(engine.models) : []
  const vis = visibleControls(recipe.model_tier, recipe.task)
  const hint = lyricsHint(recipe.lyrics)

  const patch = (p: Partial<GenerationRecipe>) => setRecipe((r) => ({ ...r, ...p }))

  const submit = async () => {
    setMessage(null)
    const problems = recipeProblems(recipe)
    if (problems.length > 0) {
      setMessage(problems.join("; "))
      return
    }
    const snap = useStudio.getState().snapshot
    if (!snap) {
      setMessage("Mở hoặc tạo project trước đã")
      return
    }
    setBusy(true)
    try {
      // 1. Đảm bảo có track.
      let trackId = snap.arrangement.tracks[0]?.id
      if (!trackId) {
        const out = await ipc.applyEdit({ op: "add_track", kind: "generated", name: "Track 1" })
        useStudio.getState().setSnapshot(out.snapshot)
        trackId = out.snapshot.arrangement.tracks[0]?.id
      }
      if (!trackId) throw new Error("không tạo được track")

      // 2. Clip rỗng với id do client sinh — job sẽ gắn take vào clip này.
      const clipId = crypto.randomUUID()
      const out = await ipc.applyEdit({
        op: "add_clip",
        track_id: trackId,
        clip_id: clipId,
        start_ms: 0,
        duration_ms: recipe.duration_s * 1000,
        source: { type: "generated" },
      })
      useStudio.getState().setSnapshot(out.snapshot)

      // 3. Submit — kết quả quay về qua event take:ready (xem App.tsx).
      const jobId = await ipc.generateSubmit(clipId, recipe)
      useStudio.getState().patchJob(jobId, { percent: 0, stage: "queued", state: "queued" })
      useStudio.getState().selectClip(clipId)
      setMessage(`Đã xếp job ${jobId.slice(0, 8)}…`)
    } catch (e) {
      setMessage(explainError(e))
    } finally {
      setBusy(false)
    }
  }

  if (!snapshot) {
    return (
      <div className="panel">
        <h3>Generate</h3>
        <p className="dim">Mở project để bắt đầu sinh nhạc.</p>
      </div>
    )
  }

  return (
    <div className="panel">
      <h3>Generate</h3>

      <label>
        Prompt
        <textarea
          rows={3}
          value={recipe.prompt}
          placeholder="cinematic orchestral, strings, taiko…"
          onChange={(e) => patch({ prompt: e.target.value })}
        />
      </label>

      <label>
        Lyrics
        <textarea
          rows={4}
          value={recipe.lyrics}
          placeholder={"[Verse]\n..."}
          onChange={(e) => patch({ lyrics: e.target.value })}
        />
        {hint && <span className="hint">{hint}</span>}
      </label>

      <div className="row">
        <label>
          Task
          <select
            value={availTasks.includes(recipe.task) ? recipe.task : (availTasks[0] ?? "")}
            disabled={availTasks.length === 0}
            title={availTasks.length === 0 ? "Engine chưa sẵn sàng" : undefined}
            onChange={(e) => patch({ task: e.target.value as TaskType })}
          >
            {availTasks.map((t) => (
              <option key={t} value={t}>
                {TASK_LABEL[t]}
              </option>
            ))}
          </select>
        </label>
        <label>
          Model
          <select
            value={
              availTiers.includes(recipe.model_tier)
                ? recipe.model_tier
                : vis.baseOnlyModelLock
                  ? "base"
                  : (availTiers[0] ?? "")
            }
            disabled={vis.baseOnlyModelLock || availTiers.length === 0}
            title={vis.baseOnlyModelLock ? "Task này chỉ chạy trên model base" : undefined}
            onChange={(e) => patch({ model_tier: e.target.value as ModelTier })}
          >
            {(vis.baseOnlyModelLock ? availTiers.filter((t) => t === "base") : availTiers).map(
              (m) => (
                <option key={m} value={m}>
                  {TIER_LABEL[m]}
                </option>
              ),
            )}
          </select>
        </label>
      </div>
      {vis.baseOnlyModelLock && (
        <span className="hint">Task này chỉ chạy trên model base — đã khoá chọn model.</span>
      )}
      {vis.repaintModelWarning && (
        <span className="hint warn">Repaint trên turbo cho chất lượng kém — khuyến nghị sft.</span>
      )}

      <div className="row">
        <label>
          Duration (s)
          <input
            type="number"
            min={10}
            max={600}
            value={recipe.duration_s}
            onChange={(e) => patch({ duration_s: Number(e.target.value) })}
          />
        </label>
        <label>
          BPM
          <input
            type="number"
            min={30}
            max={300}
            value={recipe.bpm ?? ""}
            placeholder="tự động"
            onChange={(e) => patch({ bpm: e.target.value === "" ? null : Number(e.target.value) })}
          />
        </label>
      </div>

      {vis.lmGroup && (
        <div className="row">
          <label>
            Seed
            <input
              type="number"
              value={recipe.sampling.seed ?? ""}
              placeholder="ngẫu nhiên"
              onChange={(e) =>
                setRecipe((r) => ({
                  ...r,
                  sampling: {
                    ...r.sampling,
                    seed: e.target.value === "" ? null : Number(e.target.value),
                  },
                }))
              }
            />
          </label>
          <label>
            Steps
            <input
              type="number"
              min={1}
              value={recipe.sampling.inference_steps}
              onChange={(e) =>
                setRecipe((r) => ({
                  ...r,
                  sampling: { ...r.sampling, inference_steps: Number(e.target.value) },
                }))
              }
            />
          </label>
        </div>
      )}

      <button className="primary" disabled={busy} onClick={() => void submit()}>
        {busy ? "Đang xếp job…" : "Generate"}
      </button>
      <JobList />
      {message && <p className="hint">{message}</p>}
    </div>
  )
}
