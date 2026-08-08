#!/usr/bin/env node
/**
 * collect-bench — gom stdout của `cargo bench --workspace -- --output-format bencher`
 * thành perf-report.json cho scripts/bench-perf-budget.mjs.
 *
 * Cách dùng:
 *   cargo bench --workspace -- --output-format bencher | node scripts/collect-bench.mjs
 *   ... | node scripts/collect-bench.mjs --extra bench-extra.json --out perf-report.json
 *
 * Hai nguồn số:
 *  - BENCH_MAP: bench cargo (ns/iter) → chỉ số ngân sách (đổi đơn vị qua convert).
 *  - --extra: JSON phẳng cho chỉ số đo NGOÀI cargo bench (UI fps, cold start,
 *    xrun soak, RAM idle...) — do script bench riêng của sprint tương ứng sinh ra.
 *
 * Chỉ số nào thiếu trong report → bench-perf-budget.mjs fail. Đây là cố ý:
 * gate tồn tại để buộc bench được viết, không phải để vắt kiệt nó.
 */
import fs from "node:fs"

// Ngân sách thời gian một audio callback: buffer 512 @ 48kHz ≈ 10.667ms.
const CALLBACK_BUDGET_NS = (512 / 48000) * 1e9

/** key trong perf-report.json → bench cargo tương ứng + hàm đổi từ ns. */
const BENCH_MAP = {
  // bench: mix N track 512 frame — đo thời gian trong callback (als-audio).
  audio_callback_budget_ratio: {
    bench: "bench_audio_mixer_callback",
    convert: (ns) => ns / CALLBACK_BUDGET_NS,
  },
  // bench: peaks cho 3 phút audio (als-assets).
  peaks_3min_ms: { bench: "bench_peaks_3min", convert: (ns) => ns / 1e6 },
  // bench: đường cache-hit đầy đủ (orchestrator, mock).
  cache_hit_to_audible_ms: { bench: "bench_cache_hit_path", convert: (ns) => ns / 1e6 },
}

const EXTRA_KEYS = [
  "timeline_fps",
  "xrun_30min",
  "cold_start_ms",
  "project_open_50clip_ms",
  "reroll_seed_ratio",
  "idle_ram_mb",
]

function parseArgs(argv) {
  const args = { extra: null, out: "perf-report.json" }
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--extra") args.extra = argv[++i]
    else if (argv[i] === "--out") args.out = argv[++i]
  }
  return args
}

function main() {
  const { extra, out } = parseArgs(process.argv.slice(2))
  const input = fs.readFileSync(0, "utf8")
  const report = {}

  // Dòng bencher: `test bench_name ... bench:  1,234,567 ns/iter (+/- 890)`
  const re = /^test\s+(\S+)\s+\.\.\.\s+bench:\s+([\d,]+)\s+ns\/iter/gm
  let m
  let parsed = 0
  while ((m = re.exec(input)) !== null) {
    parsed++
    const [, benchName, nsStr] = m
    const ns = Number(nsStr.replaceAll(",", ""))
    for (const [key, spec] of Object.entries(BENCH_MAP)) {
      if (benchName === spec.bench || benchName.endsWith(`::${spec.bench}`)) {
        report[key] = Number(spec.convert(ns).toFixed(4))
      }
    }
  }

  if (extra) {
    if (!fs.existsSync(extra)) {
      console.error(`--extra chỉ tới file không tồn tại: ${extra}`)
      process.exit(1)
    }
    const ext = JSON.parse(fs.readFileSync(extra, "utf8"))
    for (const k of EXTRA_KEYS) {
      if (ext[k] !== undefined) report[k] = ext[k]
    }
  }

  fs.writeFileSync(out, JSON.stringify(report, null, 2) + "\n")
  console.log(`cargo bench parse được ${parsed} dòng; ghi ${Object.keys(report).length} chỉ số → ${out}`)
  for (const [k, v] of Object.entries(report)) console.log(`  ${k}: ${v}`)

  if (parsed === 0 && !extra) {
    console.error("Không parse được dòng bench nào và không có --extra — kiểm tra lại lệnh cargo bench.")
    process.exit(1)
  }
}

main()
