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

> **Ngay sau lệnh này: `git add pnpm-lock.yaml Cargo.lock && git commit`.**
> CI dùng `--frozen-lockfile` (pnpm) và resolver kênh rc (specta) — cả hai
> lockfile đều cần commit sau lần build đầu tiên.

## 2. Kiểm chứng Rust

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

PASS: toàn bộ test xanh — gồm property test hash (NFC/NFD tiếng Việt cùng
`plan_hash`), golden buffer mixer, no-alloc, migration, contract suite của
MockProvider, orchestrator end-to-end (cache hit 2 tầng, re-roll seed).

### Điểm kiểm tra đã biết #1 — specta kênh prerelease ✅ đã xử lý

`specta` 2.x chỉ có bản `2.0.0-rc.*` trên crates.io; requirement `^2` không
match prerelease → cargo báo "failed to select a version". Repo đã chuyển
` specta / tauri-specta / specta-typescript` sang requirement `"2.0.0-rc"`.
Rủi ro còn lại: API giữa các bản rc có thể trôi (ví dụ chữ ký
`Builder::invoke_handler`). Nếu lỗi compile xuất hiện quanh `lib.rs` — tra
README của tauri-specta tại bản rc trong Cargo.lock, hoặc hạ bản:
`cargo update -p tauri-specta --precise 2.0.0-rc.<n>` rồi build lại.

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

## 4. Frontend + luồng nghiệp vụ đầu cuối

```powershell
pnpm typecheck
pnpm test
pnpm dev
```

PASS: typecheck sạch; vitest xanh (rules.test.ts). Trong app:

1. Tạo project → bấm **Generate** (MockProvider) → clip xuất hiện với
   **waveform** sau vài giây.
2. Bấm **▶** → nghe take sine; click ruler/lane → playhead nhảy đúng chỗ;
   nút **Loop** lặp toàn project.
3. **Import** file audio thật (đường dẫn tuyệt đối, cách nhau bởi `;`) →
   clip có waveform và nghe được.
4. **Export WAV** → file 24-bit xuất hiện kèm sidecar `.meta.json`.
5. Đổi backend ở header (mock/cpp/py) — cpp/py sẽ báo chưa sẵn sàng cho tới
   khi Phase 0 xong; đó là hành vi đúng, không phải lỗi.

## 5. Sau khi xanh toàn bộ

→ Phase 0: bắt đầu từ issue #3 (S-01) và #4 (S-02). Kết quả điền vào
`docs/phase0/spike-report.md`. Không mở Sprint 1 khi bảng đó còn ô trống.

## Kết quả kiểm chứng offline đã có (2026-08-09)

Phần kiểm chứng được mà không cần build Rust — đã chạy và PASS:

| Kiểm                                                                                                                                | Kết quả                          |
| ----------------------------------------------------------------------------------------------------------------------------------- | -------------------------------- |
| `tsc` strict (exactOptionalPropertyTypes, verbatimModuleSyntax) toàn bộ frontend + packages, gồm lớp playback/export/track-controls | PASS                             |
| Prettier `--check` file viết tay (đúng config repo)                                                                                 | PASS                             |
| YAML workflow `ci.yml` / `release.yml` parse                                                                                        | PASS                             |
| `scripts/collect-bench.mjs` + `bench-perf-budget.mjs` (3 chiều: đủ số pass / vượt budget fail / thiếu report fail)                  | PASS                             |
| `cargo build/test/clippy`                                                                                                           | đang chạy trên máy thật — bước 2 |
