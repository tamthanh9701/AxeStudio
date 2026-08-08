/**
 * Cache peaks phía UI: một asset chỉ fetch một lần, invalidate khi Rust bắn
 * peaks:ready (peaks được sinh lại sau render mới — nội dung đổi thì asset id
 * đổi theo, nhưng invalidate vẫn an toàn cho trường hợp ghi đè).
 */
import { ipc } from "../../ipc/client"
import type { PeakView } from "@als/bindings"

const cache = new Map<string, Promise<PeakView | null>>()

export function getPeaks(assetId: string): Promise<PeakView | null> {
  let p = cache.get(assetId)
  if (!p) {
    // zoom_level 1 (1024 spp) — cân bằng chi tiết/kích thước cho clip vài chục giây.
    p = ipc.assetPeaks(assetId, 1).catch(() => null)
    cache.set(assetId, p)
  }
  return p
}

export function invalidatePeaks(assetId: string): void {
  cache.delete(assetId)
}
