#!/usr/bin/env node
/**
 * bench:budget — đọc perf-report.json và đối chiếu ngân sách.
 *
 * Bảng ngân sách ở đây PHẢI mirror docs/perf-budget.md. Nới một chỉ số ở đây
 * mà không kèm ADR = vi phạm AGENTS.md §1.
 *
 * perf-report.json do `cargo bench --workspace` + scripts/collect-bench sinh ra
 * (job bench trên self-hosted runner có GPU).
 */
import fs from "node:fs"

const BUDGETS = {
  timeline_fps: { min: 60 },
  audio_callback_budget_ratio: { max: 0.3 },
  xrun_30min: { max: 0 },
  cold_start_ms: { max: 3000 },
  project_open_50clip_ms: { max: 1500 },
  cache_hit_to_audible_ms: { max: 200 },
  reroll_seed_ratio: { max: 0.4 },
  peaks_3min_ms: { max: 800 },
  idle_ram_mb: { max: 600 },
}

const reportPath = process.argv[2] ?? "perf-report.json"
if (!fs.existsSync(reportPath)) {
  console.error(`Không thấy ${reportPath} — bench chưa chạy, không đánh giá được.`)
  process.exit(1)
}

const report = JSON.parse(fs.readFileSync(reportPath, "utf8"))
let failed = 0
let missing = 0

for (const [key, budget] of Object.entries(BUDGETS)) {
  const actual = report[key]
  if (actual === undefined) {
    console.warn(`? ${key}: thiếu số đo trong report`)
    missing++
    continue
  }
  const ok = budget.min !== undefined ? actual >= budget.min : actual <= budget.max
  const limit = budget.min !== undefined ? `>= ${budget.min}` : `<= ${budget.max}`
  console.log(`${ok ? "✓" : "✗"} ${key}: ${actual} (ngân sách ${limit})`)
  if (!ok) failed++
}

if (missing > 0) {
  console.error(`${missing} chỉ số thiếu — bench chưa đủ, không merge.`)
  process.exit(1)
}
if (failed > 0) {
  console.error(`${failed} chỉ số VƯỢT ngân sách — không merge.`)
  process.exit(1)
}
console.log("Tất cả chỉ số trong ngân sách.")
