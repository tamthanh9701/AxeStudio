#!/usr/bin/env node
/**
 * bindings:check — CI drift gate.
 *
 * 1. Sinh lại packages/bindings/src/generated.ts từ Rust types.
 * 2. So với bản đã commit. Lệch → ai đó đổi Rust type mà quên regenerate.
 *
 * Chạy từ thư mục gốc repo.
 */
import { execSync } from "node:child_process"

execSync("cargo run -p als-desktop --bin export-bindings", { stdio: "inherit" })

try {
  execSync("git diff --exit-code -- packages/bindings/src/generated.ts", {
    stdio: "inherit",
  })
  console.log("✓ bindings khớp với Rust types")
} catch {
  console.error(
    "✗ generated.ts lệch với Rust types.\n" +
      "  Chạy `pnpm bindings:generate` rồi commit file generated.ts kèm PR của bạn.",
  )
  process.exit(1)
}
