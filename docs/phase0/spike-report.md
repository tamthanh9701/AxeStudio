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
| turbo, 30s  | 57.2 / 61.5 | ❌ | 44.6 / 18.4 | ❌ vllm không có trên Windows — server tự fallback pt (xem S-02) |
| turbo, 120s | 259.5 / 259.6 | ❌ | 127.4 / 48.8 | ❌ (fallback pt) |
| turbo, 240s | 519.3 / 519.6 | ❌ | 48.4 / 36.3 | ❌ (fallback pt) |
| sft, 30s    | 8.4 / 7.6 | ❌ | 18.4 / 20.7 | ❌ (fallback pt) |
| sft, 120s   | 131.7 / 183.0 | ❌ | 36.4 / 28.4 | ❌ (fallback pt) |
| sft, 240s   | 409.9 / 407.5 | ❌ | 38.5 / 54.8 | ❌ (fallback pt) |

**Cấu hình cột cpp:** LM-0.6B Q8_0 — giống hệt LM mà phía py tự chọn (auto
tier) → so sánh công bằng backend. VAE decode của cpp chiếm phần lớn thời
gian và tỉ lệ tuyến tính với duration (~1.7s VAE / 1s audio).

**Tham chiếu cấu hình ship mặc định của ace.cpp (LM-4B Q8_0, ~7.7GB bộ
model):** turbo 30s = 60.1/61.5 — tương đương; từ 120s trở lên sụp đổ vì
KV cache + weights tràn 8GB VRAM (WDDM spill): 120s = 212.3/**650.0**,
240s = **1846.6**/750.3; ô sft/30s với LM-4B không hoàn thành sau >45 phút
(DiT 1071s + VAE tiled treo) — bị bỏ. Kết luận: cấu hình ship mặc định của
ace.cpp KHÔNG dùng được trên GPU 8GB ở duration dài.

**Sự cố môi trường trong lúc đo:** VRAM nền desktop tăng ~3.5→8.0GB giữa
chừng làm một lượt số xấu (VAE 523s cho 30s audio); đã đóng app heavy và
đo lại toàn bộ cột cpp bên trên.

Ghi chú phương sai: pha LM của py có phương sai lớn (LM chunk sinh token
5Hz, thời gian không tỉ lệ với duration audio) — cả hai số được ghi nguyên
trạng, không làm tròn lên (issue #2).

## S-04 — Cold start / VRAM

| Backend     | Cold start (s) | Warm gen 120s (s) | Peak VRAM turbo | Peak VRAM sft | RAM host đỉnh |
| ----------- | -------------- | ----------------- | --------------- | ------------- | ------------- |
| cpp CUDA    | 5.1 (/props — model nạp lazy theo request đầu) | ≈260 (S-03, LM-0.6B) | 7767 MB | 7781 MB | 1484 / 1407 MB |
| cpp Vulkan  | ❌ | ❌ | ❌ | ❌ | ❌ |
| python pt   | (đang đo — bổ sung) | | | | |
| python vllm | ❌ — không có native Windows | — | — | — | — |

Ghi chú cpp: "cold start" chỉ tính đến `/props` 200; weights nạp lazy khi
request đầu → thời gian nhận job thật = cold start + lần gen đầu (~30–60 s
với LM-0.6B). Peak VRAM là MAX mẫu 1 Hz trong lúc render 120s (gồm cả
desktop ~0.9 GB) — sát trần 8192 MB.

Phát hiện S-04 phụ: server py với 3 slot (`ACESTEP_CONFIG_PATH{1,2,3}`)
nạp EAGER cả 3 DiT lúc startup → cạn VRAM trên GPU 8GB, VAE tiled decode
treo vĩnh viễn ở mức free ≈ 0.04 GB. Xem S-05.

## S-05 — Hot-swap `/v1/init`

| Chuyển đổi  | Thời gian (s) | VRAM đỉnh trong lúc swap |
| ----------- | ------------------------ |
| turbo → sft | 36.5 (từ lúc POST /v1/init đến khi gen 30s xác minh xong) | 7738 MB (before 6819 → after 6952 MB) |
| sft → base  | 25.0 (cùng giao thức) | 7396 MB (before 6964 → after 6792 MB) |

**Kết luận:** hot-swap **DÙNG ĐƯỢC** cho VRAM scheduler — 3 lần swap liên
tiếp không thấy VRAM tích tụ (6819 → 6952 → 6792 MB, dao động ±130 MB là
fragmentation bình thường); `/v1/init` trả lời tức thì, load chạy ngầm và
gen xác minh 30s thành công ngay sau swap.

**Phát hiện quan trọng:** cơ chế 3 slot preload (`ACESTEP_CONFIG_PATH{1,2,3}`)
nạp đồng thời 3 DiT lúc startup → trên GPU 8GB cạn kiệt VRAM (free 0.04 GB),
VAE tiled decode treo vĩnh viễn. Scheduler (WS-D) phải thiết kế theo mô hình
**1 model resident + swap trong slot**, không phải 3 slot nóng song song.

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
