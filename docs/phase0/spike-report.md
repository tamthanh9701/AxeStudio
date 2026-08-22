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
| CUDA    | ✅ (CMake 3.31 + Ninja, nvcc 13.3, 183/183 target) | ✅ `ace-server.exe` — `/props` 200, `/understand` 200, `/lm`+`/synth?wav=1` trả WAV RIFF | Log: `docs/phase0/logs/s01-cuda.txt`. Submodule pin `acestep.vst3@b04bf8a` (ggml@4d74a9a8). Lưu ý runtime: cần `cublas64_13.dll`/`cublasLt64_13.dll` (CUDA bin/x64) + shim `cudart_hybrid64.dll` (copy của cudart64_13) cạnh exe |
| Vulkan  | ❌ — chưa đo được trên máy này: Vulkan SDK installer (winget/LunarG) cần UAC elevation và bị từ chối/hết hạn 3 lần; bản zip LunarG thực chất là EXE | — | Chưa phải kết luận kỹ thuật về code — là giới hạn môi trường đo. Cần máy/cài SDK rồi chạy lại trước khi tuyên bố "NVIDIA only" |

## S-02 — Python ACE-Step 1.5 native Windows

| Mục                            | Kết quả |
| ------------------------------ | ------- |
| `uv sync` thành công           | ✅ (uv 0.12.5, Python 3.12.10 native Windows)                                                                                                          |
| `lm_backend=vllm` native       | ❌ — server log: "vLLM backend is unavailable on Windows because Triton is not installed or is incompatible. Falling back to the PyTorch backend."     |
| `lm_backend=pt` thời gian plan | LM phase (log server, load→offload trừ DiT): 28.9 / 19.0 / 13.5 s — median ≈ 19 s. Total-wall cả job (script bấm): 44.9 / 32.8 / 34.9 s — median 34.9 s |

## S-03 — Benchmark ma trận

Thời gian sinh (giây), warm model:

|             | cpp CUDA | cpp Vulkan | python pt | python vllm |
| ----------- | -------- | ---------- | --------- | ----------- |
| turbo, 30s  |          |            | 44.6 / 18.4 | ❌ vllm không có trên Windows — server tự fallback pt (xem S-02) |
| turbo, 120s |          |            | 127.4 / 48.8 | ❌ (fallback pt) |
| turbo, 240s |          |            | 48.4 / 36.3 | ❌ (fallback pt) |
| sft, 30s    |          |            | 18.4 / 20.7 | ❌ (fallback pt) |
| sft, 120s   |          |            | 36.4 / 28.4 | ❌ (fallback pt) |
| sft, 240s   |          |            | 38.5 / 54.8 | ❌ (fallback pt) |

Ghi chú phương sai: pha LM của py có phương sai lớn (LM chunk sinh token
5Hz, thời gian không tỉ lệ với duration audio) — cả hai số được ghi nguyên
trạng, không làm tròn lên (issue #2).

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
