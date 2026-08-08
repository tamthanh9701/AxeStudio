import { useState } from "react"
import type { GenerationRecipe, ModelTier, TaskType } from "@als/bindings"
import { explainError, ipc } from "../../ipc/client"
import { useStudio } from "../../state/store"
import { lyricsHint, recipeProblems, visibleControls } from "./rules"

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

const MODEL_TIERS: { value: ModelTier; label: string }[] = [
  { value: "turbo", label: "Turbo — nhanh (8 steps)" },
  { value: "sft", label: "SFT — chất lượng cao (50 steps)" },
  { value: "base", label: "Base — extract/lego/complete" },
]

const TASKS: { value: TaskType; label: string }[] = [
  { value: "text2music", label: "Text → Music" },
  { value: "repaint", label: "Repaint (sửa một vùng)" },
  { value: "cover", label: "Cover" },
  { value: "extract", label: "Extract stem" },
  { value: "lego", label: "Lego (thêm track)" },
  { value: "complete", label: "Complete" },
]

export function GeneratePanel() {
  const [recipe, setRecipe] = useState<GenerationRecipe>(defaultRecipe)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<string | null>(null)
  const snapshot = useStudio((s) => s.snapshot)
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
            value={recipe.task}
            onChange={(e) => patch({ task: e.target.value as TaskType })}
          >
            {TASKS.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Model
          <select
            value={recipe.model_tier}
            disabled={vis.baseOnlyModelLock}
            title={vis.baseOnlyModelLock ? "Task này chỉ chạy trên model base" : undefined}
            onChange={(e) => patch({ model_tier: e.target.value as ModelTier })}
          >
            {MODEL_TIERS.map((m) => (
              <option key={m.value} value={m.value}>
                {m.label}
              </option>
            ))}
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
            onChange={(e) =>
              patch({ bpm: e.target.value === "" ? null : Number(e.target.value) })
            }
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
      {message && <p className="hint">{message}</p>}
    </div>
  )
}
