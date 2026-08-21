# AxeStudio

**Local-first generative music studio cho Windows.** Một workstation soạn nhạc chạy hoàn toàn trên máy người dùng, dùng [ACE-Step 1.5](https://github.com/ace-step/ACE-Step-1.5) làm engine sinh nhạc.

> **Trạng thái: Phase 0 — Spike.** Chưa có bản cài đặt. Repo hiện tại là bộ khung + contract, chưa phải sản phẩm chạy được.

---

## Ý tưởng

ACE Studio là piano-roll vocal workstation: người dùng vẽ note MIDI + lời, engine render từng vùng. ACE-Step thì ngược lại — nó là **full-track generative engine**: đưa prompt vào, nhận cả bài ra.

AxeStudio **không** cố biến ACE-Step thành vocal synth MIDI-native. Thay vào đó nó xây một editor phi phá huỷ (non-destructive) phía trên một engine sinh nhạc, với ba nguyên tắc:

1. **Generate tạo take mới, không ghi đè.** Mọi lần sinh là một `take` bất biến, có recipe đầy đủ để tái lập.
2. **Cache theo nội dung, không theo thời gian.** Đổi seed không được phép chạy lại pha LM.
3. **Audio engine là native Rust.** Web Audio API không đủ cho multi-track realtime.

## Kiến trúc

```
┌─────────────────────────────────────────────────────────┐
│  apps/desktop  —  React 19 + PixiJS  (WebView)          │
└───────────────────────┬─────────────────────────────────┘
                        │ Tauri IPC (command + event)
┌───────────────────────┴─────────────────────────────────┐
│  src-tauri  —  Rust host process                        │
│  ┌──────────┬───────────┬──────────┬─────────────────┐  │
│  │ als-audio│als-project│als-assets│ als-orchestrator│  │
│  │ (realtime│ (SQLite   │(content- │ (queue + cache  │  │
│  │  thread) │  + WAL)   │ addressed│  2 tầng)        │  │
│  └──────────┴───────────┴──────────┴────────┬────────┘  │
└───────────────────────────────────────────┬─┴───────────┘
                                            │ trait RenderProvider
                        ┌───────────────────┴──────────────────┐
                        │                                      │
              als-provider-cpp                        als-provider-py
              (acestep.cpp / GGUF)                    (acestep-api / 8001)
```

Audio engine **không phụ thuộc** crate nào khác trong workspace. Đây là ràng buộc cố ý — nó phải test được độc lập và không bao giờ bị kéo vào một dependency có allocation.

## Cấu trúc repo

```txt
apps/desktop/          React 19 + TS + Vite, shell Tauri 2
crates/
  als-core/            kiểu chung, id, error, canonicalize + hash
  als-audio/           audio engine — KHÔNG phụ thuộc crate khác
  als-project/         SQLite + manifest + migration
  als-assets/          content-addressed store, peaks mipmap
  als-orchestrator/    job queue, cache 2 tầng, VRAM scheduler
  als-provider/        trait RenderProvider + contract tests + mock
  als-provider-cpp/    client cho ace-server (acestep.cpp)
  als-provider-py/     client cho acestep-api (Python)
  als-media/           decode, resample, loudness
packages/
  bindings/            TS types SINH TỰ ĐỘNG từ Rust — không sửa tay
  timeline/            PixiJS renderer, test được độc lập
  ui/                  design system
docs/adr/              Architecture Decision Records
scripts/               bench, model-fetch, release
```

## Lộ trình

| Phase | Tuần    | Nội dung                                        | Bàn giao                        |
| ----- | ------- | ----------------------------------------------- | ------------------------------- |
| **0** | W1–W2   | Spike kỹ thuật, đo số thật                      | Spike report + chốt backend     |
| **1** | W3–W16  | MVP — Local Generative Studio                   | `.msi` ký số: generate → export |
| **2** | W17–W26 | Controlled editing (repaint, stems, understand) | Beta công khai                  |
| **3** | W27–W34 | LoRA library + training                         | Bản 1.0                         |
| **4** | TBD     | Vocal synth provider                            | Phụ thuộc engine bên thứ ba     |

**Không bắt đầu Phase 1 khi Phase 0 chưa có số.** Toàn bộ mô hình UX phụ thuộc đúng hai con số: _cold start_ và _warm inference_. Xem [`docs/phase0/`](docs/phase0/).

## Bắt đầu

### Yêu cầu

- Windows 11 x64
- Rust stable (xem `rust-toolchain.toml`)
- Node 20+ và pnpm 9+
- NVIDIA GPU ≥ 8GB VRAM để chạy engine thật (mock provider không cần GPU)

### Chạy dev

```bash
pnpm install
pnpm dev            # Tauri dev, dùng MockProvider — không cần GPU
```

### Kiểm tra như CI

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck && pnpm test
pnpm bindings:check   # fail nếu TS types lệch với Rust
```

## Ngân sách hiệu năng

Đây là **điều kiện merge**, không phải mục tiêu tham khảo.

| Chỉ số                   | Ngân sách                                     |
| ------------------------ | --------------------------------------------- |
| Timeline scroll/zoom     | ≥ 60fps với 24 track / 200 clip               |
| Audio callback           | < 30% ngân sách ở buffer 512 @ 48kHz (≈3.5ms) |
| Xrun                     | 0 trong 30 phút phát liên tục                 |
| Cold start → UI sẵn sàng | < 3s                                          |
| Mở project 50 clip       | < 1.5s                                        |
| Cache hit → nghe được    | < 200ms                                       |
| Re-roll seed (plan hit)  | ≤ 40% thời gian sinh đầy đủ                   |
| Peaks cho 3 phút audio   | < 800ms                                       |

Chi tiết: [`docs/perf-budget.md`](docs/perf-budget.md).

## Đóng góp

Đọc [`AGENTS.md`](AGENTS.md) trước khi viết dòng code đầu tiên — kể cả khi bạn là người, không phải AI agent. File đó chứa các ràng buộc cứng (đặc biệt là ở `als-audio`) mà vi phạm sẽ không được merge.

## Giấy phép

Chưa chốt. Xem quyết định treo #4 trong build plan. Cho đến khi có file `LICENSE`, mặc định là _all rights reserved_.

ACE-Step 1.5 và acestep.cpp là MIT — AxeStudio gọi chúng qua process riêng, không link tĩnh.
