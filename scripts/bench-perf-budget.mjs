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

// Chỉ số CHƯA có script đo (sprint tương ứng chưa đến — collector đọc qua
// --extra). Được phép thiếu mà không fail, nhưng KHÔNG bị xoá âm thầm:
// sprint nào viết xong script đo thì xoá key khỏi danh sách này trong
// chính PR đó. Quyết định 2026-08-24 sau khi bench job chạy thật lần đầu
// (runner axe-gpu-runner) — gate phải xanh trên những gì đang tồn tại.
const PENDING_KEYS = new Set([
  "timeline_fps", // Sprint timeline (Pixi ticker)
  "xrun_30min", // cần thiết bị audio thật + soak script (S-08 đã chứng minh thủ công)
  "cold_start_ms", // Sprint first-run experience
  "project_open_50clip_ms", // cần fixture project 50 clip
  "reroll_seed_ratio", // orchestrator bench — làm cùng ticket cache-hit nếu profiler kêu
  "idle_ram_mb", // cần app build release chạy 60s idle
])

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
    if (PENDING_KEYS.has(key)) {
      console.log(`⏳ ${key}: pending — sprint tương ứng chưa có script đo`)
    } else {
      console.warn(`? ${key}: thiếu số đo trong report`)
      missing++
    }
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
