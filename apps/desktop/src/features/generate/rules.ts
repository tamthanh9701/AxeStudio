/**
 * Quy tắc ẩn/hiện control của Generation panel (ticket ALS-F05).
 * PURE function — component chỉ đọc kết quả, không tự suy luận.
 *
 * Nguyên tắc: hiện một control mà kéo không đổi gì là lỗi UX nghiêm trọng.
 */
import type {
  Capability,
  GenerationRecipe,
  ModelDescriptor,
  ModelTier,
  TaskType,
} from "@als/bindings"

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

// ---------- ALS-F05 (#10): danh sách khả dụng đọc từ engine_status ----------
//
// Nguồn sự thật duy nhất là `EngineStatus.capabilities` / `.models` — CẤM
// hardcode danh sách task/model trong component (AGENTS §7). Mapping dưới
// đây mirror `Capability::for_task` ở als-core; giá trị literal khớp serde
// snake_case và được typecheck chéo với union sinh ra trong generated.ts.

/** Task → capability tương ứng (mirror `Capability::for_task`). */
const TASK_CAPABILITY: Record<TaskType, Capability> = {
  text2music: "text2_music",
  cover: "cover",
  repaint: "repaint",
  lego: "lego",
  extract: "extract",
  complete: "complete",
}

export const ALL_TASKS = Object.keys(TASK_CAPABILITY) as TaskType[]

/** Thứ tự ưu tiên hiển thị tier trong dropdown. */
const TIER_ORDER: ModelTier[] = [
  "turbo",
  "xl_turbo",
  "sft",
  "xl_sft",
  "base",
  "xl_base",
]

/**
 * Task provider active thực sự làm được. Exported vì GeneratePanel và test
 * cùng dùng một định nghĩa "khả dụng" — đây là ranh giới domain, không phải
 * wrapper one-line.
 */
export function availableTasks(capabilities: readonly Capability[]): TaskType[] {
  return ALL_TASKS.filter((t) => capabilities.includes(TASK_CAPABILITY[t]))
}

/**
 * Tier có model thật phía provider, sắp theo TIER_ORDER để dropdown ổn định
 * khi backend đổi danh sách trả về.
 */
export function availableTiers(models: readonly ModelDescriptor[]): ModelTier[] {
  const tiers = models.map((m) => m.tier)
  return TIER_ORDER.filter((t) => tiers.includes(t))
}
