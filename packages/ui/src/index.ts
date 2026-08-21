/**
 * @als/ui — design tokens. Mở rộng thành component library ở Sprint 4–6.
 * Framework-free: component React nằm ở apps/desktop, token nằm đây để
 * timeline canvas (Pixi) và DOM dùng CHUNG một bộ màu.
 */

export const colors = {
  bg: "#141417",
  bgPanel: "#1c1c21",
  bgHover: "#26262d",
  border: "#33333b",
  text: "#e8e8ec",
  textDim: "#9a9aa5",
  accent: "#7c6cf2",
  accentHover: "#9385ff",
  danger: "#e5534b",
  warn: "#d9a13b",
  ok: "#4ec98e",
  clipGenerated: "#4a3f8f",
  clipImported: "#2f5f6f",
  playhead: "#ff5d5d",
} as const

export const spacing = {
  xs: 4,
  sm: 8,
  md: 12,
  lg: 16,
  xl: 24,
} as const

export const typography = {
  fontFamily: '"Inter", "Segoe UI", system-ui, -apple-system, sans-serif',
  monoFamily: '"JetBrains Mono", "Cascadia Code", Consolas, monospace',
  sizeSm: 12,
  sizeMd: 14,
  sizeLg: 18,
} as const

export const layout = {
  trackHeaderWidth: 200,
  trackHeight: 72,
  rulerHeight: 28,
  inspectorWidth: 340,
} as const
