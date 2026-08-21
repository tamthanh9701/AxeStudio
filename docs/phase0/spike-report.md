# Phase 0 — Spike Report

> **Trạng thái: CHƯA ĐIỀN.** Không bắt đầu Sprint 1 khi bảng này còn ô trống.
>
> Mỗi ô phải có số đo thực tế kèm cấu hình máy đo (GPU, VRAM, RAM, driver). Đoán = chưa đo.

## Máy đo

| Hạng mục | Giá trị                                 |
| -------- | --------------------------------------- |
| GPU      | NVIDIA GeForce RTX 3070 (Ampere, GA104) |
| VRAM     | 8192 MiB GDDR6                          |
| RAM      | 32 GB (31.8 GB usable)                  |
| Driver   | NVIDIA 596.49 (CUDA 13.2)               |
| OS build | Windows 11 Pro build 26200              |

## S-01 — Build acestep.cpp

| Backend | Build được? | Binary chạy? | Ghi chú |
| ------- | ----------- | ------------ | ------- |
| CUDA    |             |              |         |
| Vulkan  |             |              |         |

## S-02 — Python ACE-Step 1.5 native Windows

| Mục                            | Kết quả |
| ------------------------------ | ------- |
| `uv sync` thành công           |         |
| `lm_backend=vllm` native       | ✅ / ❌ |
| `lm_backend=pt` thời gian plan |         |

## S-03 — Benchmark ma trận

Thời gian sinh (giây), warm model:

|             | cpp CUDA | cpp Vulkan | python pt | python vllm |
| ----------- | -------- | ---------- | --------- | ----------- |
| turbo, 30s  |          |            |           |             |
| turbo, 120s |          |            |           |             |
| turbo, 240s |          |            |           |             |
| sft, 30s    |          |            |           |             |
| sft, 120s   |          |            |           |             |
| sft, 240s   |          |            |           |             |

## S-04 — Cold start / VRAM

| Backend     | Cold start (s) | Warm gen 120s (s) | Peak VRAM turbo | Peak VRAM sft | RAM host đỉnh |
| ----------- | -------------- | ----------------- | --------------- | ------------- | ------------- |
| cpp CUDA    |                |                   |                 |               |               |
| cpp Vulkan  |                |                   |                 |               |               |
| python pt   |                |                   |                 |               |               |
| python vllm |                |                   |                 |               |               |

## S-05 — Hot-swap `/v1/init`

| Chuyển đổi  | Thời gian (s) | VRAM đỉnh trong lúc swap |
| ----------- | ------------- | ------------------------ |
| turbo → sft |               |                          |
| sft → base  |               |                          |

## S-06 — Repaint seam (sft)

| Crossfade | Nghe được seam? | Ghi chú |
| --------- | --------------- | ------- |
| 0ms       |                 |         |
| 50ms      |                 |         |
| 150ms     |                 |         |
| 300ms     |                 |         |

**Chốt:** ___ ms

## S-07 — Extract vs Demucs v4

| Bài | vocals (ACE/Demucs) | drums | bass | other |
| --- | ------------------- | ----- | ---- | ----- |
| 1   | /                   | /     | /    | /     |
| 2   | /                   | /     | /    | /     |
| 3   | /                   | /     | /    | /     |
| 4   | /                   | /     | /    | /     |
| 5   | /                   | /     | /    | /     |

**Kết luận:**

## S-08 — Rust audio prototype

| Buffer | xrun trong 30 phút | Latency đo được |
| ------ | ------------------ | --------------- |
| 256    |                    |                 |
| 512    |                    |                 |
| 1024   |                    |                 |

## Kill criteria — đánh dấu sau khi đo

| Điều kiện                               | Kích hoạt? | Hệ quả bắt buộc                                                  |
| --------------------------------------- | ---------- | ---------------------------------------------------------------- |
| Warm gen 120s > 30s trên GPU mục tiêu   |            | Bỏ UX "như nhạc cụ" → render queue + notification                |
| Cold start > 60s                        |            | Warm model khi mở project + màn "Engine warming" có tiến độ thật |
| vLLM không native **và** pt chậm hơn 2× |            | cpp là backend duy nhất v1                                       |
| Vulkan build fail / sai output          |            | "NVIDIA only" ở v1                                               |
| Rust audio xrun ở buffer 512            |            | Buffer mặc định 1024 hoặc xem lại streaming                      |
| extract tệ hơn Demucs rõ rệt            |            | Demucs làm provider riêng ở Phase 2                              |

## Kết luận Phase 0

- **Backend mặc định:**
- **ADR-001 chuyển sang:** Accepted / Rejected
- **Ngày chốt:**
