/**
 * Cảnh báo vận hành engine (ADR-002 amendment "cpp-first cho 8GB").
 * PURE function — App chỉ đọc kết quả, không tự suy luận.
 */
import type { EngineStatus } from "@als/bindings"

/**
 * Free VRAM tối thiểu để VAE decode chạy GPU thay vì CPU-tiled.
 * Evidence máy đo 8GB (issue #14): server cần ≥2.6GB free trước VAE;
 * thiếu → CPU path có thể chậm vô hạn.
 */
export const VRAM_SAFE_MIN_MB = 2600

export interface VramWarning {
  freeMb: number
  message: string
}

/**
 * Cảnh báo khi free VRAM dưới ngưỡng an toàn. Engine không báo được số
 * (backend py luôn None) → KHÔNG cảnh báo — chỉ cảnh báo trên dữ liệu thật,
 * không phỏng đoán.
 *
 * Copy KHÔNG được hứa hẹn first-gen nhanh cho backend Python trên 8GB
 * (ADR-002: đường đó không khả dụng tin cậy).
 */
export function vramWarning(engine: EngineStatus | null): VramWarning | null {
  const free = engine?.vram_free_mb
  if (free === null || free === undefined) return null
  if (free >= VRAM_SAFE_MIN_MB) return null
  return {
    freeMb: free,
    message:
      `VRAM trống ${free} MB < ${VRAM_SAFE_MIN_MB} MB — backend Python có thể rơi ` +
      "CPU-VAE rất chậm hoặc không hoàn thành. Khuyến nghị: backend cpp (acestep.cpp) " +
      "cho Text → Music / Cover, hoặc đóng app heavy rồi thử lại.",
  }
}
