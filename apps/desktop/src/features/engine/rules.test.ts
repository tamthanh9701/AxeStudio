import { describe, expect, it } from "vitest"
import type { EngineStatus } from "@als/bindings"
import { VRAM_SAFE_MIN_MB, vramWarning } from "./rules"

function engine(vramFreeMb: number | null): EngineStatus {
  return {
    backend: "py",
    ready: true,
    warm_models: [],
    vram_free_mb: vramFreeMb,
    queue_depth: 0,
    capabilities: ["text2_music"],
    models: [],
  } as unknown as EngineStatus
}

describe("vramWarning — preflight theo ADR-002 amendment", () => {
  it("engine chưa poll xong → không cảnh báo (không phỏng đoán)", () => {
    expect(vramWarning(null)).toBeNull()
  })

  it("backend không báo được free VRAM (None) → không cảnh báo", () => {
    expect(vramWarning(engine(null))).toBeNull()
  })

  it("free ≥ ngưỡng → không cảnh báo", () => {
    expect(vramWarning(engine(VRAM_SAFE_MIN_MB))).toBeNull()
    expect(vramWarning(engine(8192))).toBeNull()
  })

  it("free < ngưỡng → cảnh báo kèm số liệu và khuyến nghị cpp", () => {
    const w = vramWarning(engine(430))
    expect(w).not.toBeNull()
    expect(w?.freeMb).toBe(430)
    expect(w?.message).toContain("430 MB")
    expect(w?.message).toContain("2600 MB")
    expect(w?.message).toContain("cpp")
  })
})
