/**
 * Quy tắc ẩn/hiện control của Generation panel (ticket ALS-F05).
 * PURE function — component chỉ đọc kết quả, không tự suy luận.
 *
 * Nguyên tắc: hiện một control mà kéo không đổi gì là lỗi UX nghiêm trọng.
 */
import type { GenerationRecipe, ModelTier, TaskType } from "@als/bindings"

export interface ControlVisibility {
  /** guidance_scale / shift chỉ có tác dụng trên base (Model Zoo). */
  guidanceScale: boolean
  shift: boolean
  /** thinking bị server bỏ qua cho cover/repaint/extract → ẩn nhóm LM. */
  lmGroup: boolean
  /** extract/lego/complete chỉ chạy trên base — khoá chọn model, hiện lý do. */
  baseOnlyModelLock: boolean
  /** repaint khuyến nghị sft; chọn turbo → cảnh báo. */
  repaintModelWarning: boolean
}

export function isTurbo(tier: ModelTier): boolean {
  return tier === "turbo" || tier === "xl_turbo"
}

export function isBaseTier(tier: ModelTier): boolean {
  return tier === "base" || tier === "xl_base"
}

export function visibleControls(modelTier: ModelTier, task: TaskType): ControlVisibility {
  const lmIgnored = task === "cover" || task === "repaint" || task === "extract"
  const baseOnly = task === "extract" || task === "lego" || task === "complete"
  return {
    guidanceScale: !isTurbo(modelTier),
    shift: !isTurbo(modelTier),
    lmGroup: !lmIgnored,
    baseOnlyModelLock: baseOnly,
    repaintModelWarning: task === "repaint" && isTurbo(modelTier),
  }
}

/** Gợi ý dưới ô lyrics theo đúng ngữ nghĩa engine. */
export function lyricsHint(lyrics: string): string | null {
  const t = lyrics.trim()
  if (t === "") return "Để trống → LM sẽ tự sinh lời"
  if (t === "[Instrumental]") return "Không có vocal (chuỗi engine đã train)"
  return null
}

/** Ràng buộc giá trị — mirror validate() ở als-core. UI chặn sớm, Rust chặn cuối. */
export function recipeProblems(r: GenerationRecipe): string[] {
  const problems: string[] = []
  if (r.duration_s < 10 || r.duration_s > 600) problems.push("Duration phải trong 10–600 giây")
  if (r.bpm !== null && (r.bpm < 30 || r.bpm > 300)) problems.push("BPM phải trong 30–300")
  if (r.time_signature !== null && ![2, 3, 4, 6].includes(r.time_signature))
    problems.push("Time signature chỉ nhận 2/3/4/6")
  if (r.sampling.batch_size < 1 || r.sampling.batch_size > 8) problems.push("Batch size tối đa 8")
  const maxSteps = isTurbo(r.model_tier) ? 20 : 200
  if (r.sampling.inference_steps < 1 || r.sampling.inference_steps > maxSteps)
    problems.push(`Steps phải trong 1–${maxSteps} cho model này`)
  if (r.task === "repaint" && (r.source_audio === null || r.repaint_range_ms === null))
    problems.push("Repaint cần audio nguồn và vùng repaint")
  if (["cover", "extract", "lego"].includes(r.task) && r.source_audio === null)
    problems.push("Task này cần audio nguồn")
  return problems
}
