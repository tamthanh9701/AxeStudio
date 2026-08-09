/**
 * Vẽ waveform từ peak pairs vào canvas — dùng chung cho clip trên timeline
 * và mini-waveform trong take rack.
 */
export function drawWaveform(
  canvas: HTMLCanvasElement,
  pairs: [number, number][],
  color = "#c9c4ff",
): void {
  const ctx = canvas.getContext("2d")
  if (!ctx) return
  const w = canvas.width
  const h = canvas.height
  if (w <= 0 || h <= 0) return
  ctx.clearRect(0, 0, w, h)
  ctx.fillStyle = color
  const mid = h / 2
  const n = pairs.length
  if (n === 0) return
  for (let x = 0; x < w; x++) {
    const i = Math.min(n - 1, Math.floor((x / w) * n))
    const [lo, hi] = pairs[i] ?? [0, 0]
    const y1 = mid - hi * (mid - 1)
    const y2 = mid - lo * (mid - 1)
    ctx.fillRect(x, y1, 1, Math.max(1, y2 - y1))
  }
}
