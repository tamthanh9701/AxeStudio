import { describe, expect, it } from "vitest"
import {
  availableTasks,
  availableTiers,
  isTurbo,
  lyricsHint,
  recipeProblems,
  visibleControls,
} from "./rules"
import type { GenerationRecipe, ModelDescriptor } from "@als/bindings"

describe("visibleControls — 6 tổ hợp model × task (ALS-F05)", () => {
  it("turbo + text2music: ẩn guidance/shift, hiện nhóm LM", () => {
    const v = visibleControls("turbo", "text2music")
    expect(v.guidanceScale).toBe(false)
    expect(v.shift).toBe(false)
    expect(v.lmGroup).toBe(true)
    expect(v.baseOnlyModelLock).toBe(false)
  })

  it("base + text2music: hiện guidance/shift", () => {
    const v = visibleControls("base", "text2music")
    expect(v.guidanceScale).toBe(true)
    expect(v.shift).toBe(true)
  })

  it("sft + repaint: hiện mọi thứ trừ nhóm LM, không cảnh báo model", () => {
    const v = visibleControls("sft", "repaint")
    expect(v.lmGroup).toBe(false)
    expect(v.repaintModelWarning).toBe(false)
  })

  it("turbo + repaint: cảnh báo nên dùng sft", () => {
    const v = visibleControls("turbo", "repaint")
    expect(v.repaintModelWarning).toBe(true)
  })

  it("bất kỳ tier nào + extract: khoá model về base", () => {
    expect(visibleControls("turbo", "extract").baseOnlyModelLock).toBe(true)
    expect(visibleControls("sft", "lego").baseOnlyModelLock).toBe(true)
    expect(visibleControls("xl_sft", "complete").baseOnlyModelLock).toBe(true)
  })

  it("cover: ẩn nhóm LM (thinking bị server bỏ qua)", () => {
    expect(visibleControls("turbo", "cover").lmGroup).toBe(false)
  })
})

describe("lyricsHint", () => {
  it("lyrics rỗng → gợi ý LM tự sinh", () => {
    expect(lyricsHint("")).toMatch(/LM sẽ tự sinh/)
    expect(lyricsHint("   ")).toMatch(/LM sẽ tự sinh/)
  })
  it("[Instrumental] → badge không vocal", () => {
    expect(lyricsHint("[Instrumental]")).toMatch(/Không có vocal/)
  })
  it("lyrics thường → không hint", () => {
    expect(lyricsHint("[Verse]\\nXin chào")).toBeNull()
  })
})

describe("recipeProblems", () => {
  const okRecipe = (): GenerationRecipe => ({
    prompt: "lofi",
    lyrics: "",
    duration_s: 30,
    bpm: 90,
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
      batch_size: 2,
    },
    provider_overrides: { lm_backend: null, lm_model: null, extra: {} },
  })

  it("recipe hợp lệ → không problem", () => {
    expect(recipeProblems(okRecipe())).toEqual([])
  })

  it("duration 700 bị chặn", () => {
    const r = okRecipe()
    r.duration_s = 700
    expect(recipeProblems(r).join()).toMatch(/10–600/)
  })

  it("turbo steps 50 bị chặn (max 20), base thì OK", () => {
    const r = okRecipe()
    r.sampling.inference_steps = 50
    expect(recipeProblems(r).length).toBeGreaterThan(0)
    r.model_tier = "base"
    expect(recipeProblems(r)).toEqual([])
  })

  it("repaint thiếu source_audio bị chặn", () => {
    const r = okRecipe()
    r.task = "repaint"
    r.model_tier = "sft"
    expect(recipeProblems(r).join()).toMatch(/Repaint/)
  })
})

describe("isTurbo", () => {
  it("nhận cả xl_turbo", () => {
    expect(isTurbo("xl_turbo")).toBe(true)
    expect(isTurbo("sft")).toBe(false)
  })
})

describe("availableTasks/availableTiers — capability-driven (issue #10)", () => {
  const CAPS = {
    text2music: "text2_music",
    cover: "cover",
    repaint: "repaint",
    extract: "extract",
  } as const

  it("chỉ hiện task provider tuyên bố", () => {
    const tasks = availableTasks([CAPS.text2music, CAPS.cover])
    expect(tasks).toEqual(["text2music", "cover"])
  })

  it("provider đủ 6 task → đủ 6 task khả dụng", () => {
    const all = [
      CAPS.text2music,
      CAPS.cover,
      CAPS.repaint,
      "lego",
      CAPS.extract,
      "complete",
    ] as const
    expect(availableTasks([...all])).toHaveLength(6)
  })

  it("engine chưa poll xong (rỗng) → không task nào, panel phải disable", () => {
    expect(availableTasks([])).toEqual([])
  })

  it("tiers theo thứ tự ưu tiên turbo→sft→base bất kể thứ tự models", () => {
    const models = [
      { tier: "base" },
      { tier: "turbo" },
      { tier: "sft" },
    ] as unknown as ModelDescriptor[]
    expect(availableTiers(models)).toEqual(["turbo", "sft", "base"])
  })

  it("model lạ (tier ngoài danh sách) bị bỏ qua", () => {
    const models = [{ tier: "xl_turbo" }] as unknown as ModelDescriptor[]
    expect(availableTiers(models)).toEqual(["xl_turbo"])
  })
})

describe("recipeProblems — chặn giá trị ngoài ngân sách", () => {
  it("duration 700 bị chặn kèm thông báo (acceptance ALS-F05)", () => {
    const r: GenerationRecipe = {
      prompt: "lofi",
      lyrics: "",
      duration_s: 700,
      bpm: null,
      key_scale: null,
      time_signature: null,
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
    expect(recipeProblems(r)).toContainEqual(expect.stringContaining("Duration"))
  })
})
