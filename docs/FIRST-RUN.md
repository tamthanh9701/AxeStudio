# First Run — build lần đầu sau khi clone

Checklist này là cách nhanh nhất biết scaffold có đứng trên máy bạn không.
Mỗi bước ghi rõ PASS trông như thế nào và phải làm gì khi FAIL.

## 0. Yêu cầu

- Windows 11 x64, Rust stable (`rustup`), Node 20+, pnpm 9+
- GPU NVIDIA không bắt buộc cho bước này — MockProvider chạy không cần GPU

## 1. Clone + cài đặt

```powershell
git clone https://github.com/tamthanh9701/AxeStudio
cd AxeStudio
pnpm install
```

> **Ngay sau lệnh này: `git add pnpm-lock.yaml && git commit`.** CI dùng
> `--frozen-lockfile`; repo chưa có lockfile vì file này chỉ sinh ra được từ
> một lần install thật. Quên bước này = mọi job `web` trên CI fail.

## 2. Kiểm chứng Rust

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

PASS: toàn bộ test xanh — gồm property test hash (NFC/NFD tiếng Việt cùng
`plan_hash`), golden buffer mixer, no-alloc, migration, contract suite của
MockProvider, orchestrator end-to-end (cache hit 2 tầng, re-roll seed).

### Điểm kiểm tra đã biết #1 — `specta-typescript`

Nếu lỗi resolve `specta-typescript` ở `apps/desktop/src-tauri/Cargo.toml`:
version đó không khớp specta 2 hiện tại. `cargo update -p specta` rồi tra
[docs.rs/specta-typescript](https://docs.rs) chọn bản tương thích, sửa đúng
một dòng trong `[dev-dependencies]`.

### Điểm kiểm tra đã biết #2 — `cpal::Stream` Send

Nếu compiler báo `Engine` không `Send`/`Sync` (qua `AppState`): đừng hack
`unsafe impl`. Engine chuyển sang thread riêng + channel — pattern và lý do
đã ghi trong comment ở `apps/desktop/src-tauri/src/state.rs`.

## 3. Bindings TypeScript

```powershell
pnpm bindings:generate
git diff --exit-code packages/bindings/src/generated.ts
```

PASS: diff rỗng (placeholder viết tay khớp output thật) hoặc diff nhỏ thì
commit bản generate **thay thế** placeholder — từ đây file này là
auto-generated thật sự.

## 4. Frontend

```powershell
pnpm typecheck
pnpm test
pnpm dev
```

PASS: typecheck sạch; vitest xanh (rules.test.ts); app mở ra, tạo project,
bấm Generate → clip xuất hiện → take sine nghe được (MockProvider).

## 5. Sau khi xanh toàn bộ

→ Phase 0: bắt đầu từ issue #3 (S-01) và #4 (S-02). Kết quả điền vào
`docs/phase0/spike-report.md`. Không mở Sprint 1 khi bảng đó còn ô trống.

## Kết quả kiểm chứng offline đã có (2026-08-09)

Phần kiểm chứng được mà không cần build Rust — đã chạy và PASS:

| Kiểm | Kết quả |
| --- | --- |
| `tsc` strict (exactOptionalPropertyTypes, verbatimModuleSyntax) toàn bộ frontend + packages | PASS |
| Prettier `--check` file viết tay | PASS |
| YAML workflow `ci.yml` / `release.yml` parse | PASS |
| `scripts/collect-bench.mjs` + `bench-perf-budget.mjs` (3 chiều: đủ số pass / vượt budget fail / thiếu report fail) | PASS |
| `cargo build/test/clippy` | chưa chạy — cần máy thật (bước 2) |
