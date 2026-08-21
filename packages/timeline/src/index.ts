/**
 * @als/timeline — PixiJS renderer cho arrangement timeline.
 *
 * STUB cho Sprint 4 (WS-E). Sprint 1–3: app dùng render HTML đơn giản trong
 * features/timeline; package này tồn tại để chốt ranh giới + deps sớm.
 *
 * Yêu cầu khi hiện thực (plan §S4, perf-budget §9):
 * - 60fps với 24 track / 200 clip, zoom + scroll liên tục (đo bằng Pixi ticker).
 * - Waveform vẽ từ peaks mipmap (asset_peaks), chọn mức theo zoom.
 * - Playhead set từ NGOÀI qua setPlayheadMs trong rAF — renderer không tự poll.
 */
import { Application } from "pixi.js"
import type { Arrangement } from "@als/bindings"

export interface TimelineViewport {
  pxPerMs: number
  scrollXMs: number
  scrollYTracks: number
}

export class TimelineRenderer {
  private app: Application | null = null

  async mount(canvas: HTMLCanvasElement): Promise<void> {
    const app = new Application()
    await app.init({
      canvas,
      antialias: true,
      backgroundAlpha: 0,
      // exactOptionalPropertyTypes cấm truyền `undefined` tường minh vào
      // optional prop — chỉ set resizeTo khi parent tồn tại.
      ...(canvas.parentElement ? { resizeTo: canvas.parentElement } : {}),
    })
    this.app = app
  }

  /** TODO(S4): vẽ track lane / clip / ruler / grid / playhead từ Arrangement. */
  setArrangement(_arrangement: Arrangement): void {}

  /** TODO(S4): di chuyển playhead — gọi mỗi rAF với giá trị từ transport_position. */
  setPlayheadMs(_ms: number): void {}

  /** TODO(S4): zoom giữ điểm neo dưới con trỏ. */
  setViewport(_vp: TimelineViewport): void {}

  get mounted(): boolean {
    return this.app !== null
  }

  destroy(): void {
    this.app?.destroy()
    this.app = null
  }
}
