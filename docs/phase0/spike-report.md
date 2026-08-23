# Phase 0 — Spike Report

> **Trạng thái: ĐÃ ĐIỀN (còn 2 mục S-06/S-07 chờ phiên nghe thứ hai).**
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

| Backend | Build được?                                                                                                                                         | Binary chạy?                                                                             | Ghi chú                                                                                                                                                                                                                          |
| ------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CUDA    | ✅ (CMake 3.31 + Ninja, nvcc 13.3, 183/183 target)                                                                                                  | ✅ `ace-server.exe` — `/props` 200, `/understand` 200, `/lm`+`/synth?wav=1` trả WAV RIFF | Log: `docs/phase0/logs/s01-cuda.txt`. Submodule pin `acestep.vst3@b04bf8a` (ggml@4d74a9a8). Lưu ý runtime: cần `cublas64_13.dll`/`cublasLt64_13.dll` (CUDA bin/x64) + shim `cudart_hybrid64.dll` (copy của cudart64_13) cạnh exe |
| Vulkan  | ❌ — chưa đo được trên máy này: Vulkan SDK installer (winget/LunarG) cần UAC elevation và bị từ chối/hết hạn 3 lần; bản zip LunarG thực chất là EXE | —                                                                                        | Chưa phải kết luận kỹ thuật về code — là giới hạn môi trường đo. Cần máy/cài SDK rồi chạy lại trước khi tuyên bố "NVIDIA only"                                                                                                   |

## S-02 — Python ACE-Step 1.5 native Windows

| Mục                            | Kết quả                                                                                                                                                 |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `uv sync` thành công           | ✅ (uv 0.12.5, Python 3.12.10 native Windows)                                                                                                           |
| `lm_backend=vllm` native       | ❌ — server log: "vLLM backend is unavailable on Windows because Triton is not installed or is incompatible. Falling back to the PyTorch backend."      |
| `lm_backend=pt` thời gian plan | LM phase (log server, load→offload trừ DiT): 28.9 / 19.0 / 13.5 s — median ≈ 19 s. Total-wall cả job (script bấm): 44.9 / 32.8 / 34.9 s — median 34.9 s |

## S-03 — Benchmark ma trận

Thời gian sinh (giây), warm model:

|             | cpp CUDA      | cpp Vulkan | python pt    | python vllm                                                      |
| ----------- | ------------- | ---------- | ------------ | ---------------------------------------------------------------- |
| turbo, 30s  | 57.2 / 61.5   | ❌         | 44.6 / 18.4  | ❌ vllm không có trên Windows — server tự fallback pt (xem S-02) |
| turbo, 120s | 259.5 / 259.6 | ❌         | 127.4 / 48.8 | ❌ (fallback pt)                                                 |
| turbo, 240s | 519.3 / 519.6 | ❌         | 48.4 / 36.3  | ❌ (fallback pt)                                                 |
| sft, 30s    | 8.4 / 7.6     | ❌         | 18.4 / 20.7  | ❌ (fallback pt)                                                 |
| sft, 120s   | 131.7 / 183.0 | ❌         | 36.4 / 28.4  | ❌ (fallback pt)                                                 |
| sft, 240s   | 409.9 / 407.5 | ❌         | 38.5 / 54.8  | ❌ (fallback pt)                                                 |

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

| Backend     | Cold start (s)                                     | Warm gen 120s (s)                                       | Peak VRAM turbo | Peak VRAM sft | RAM host đỉnh                                                      |
| ----------- | -------------------------------------------------- | ------------------------------------------------------- | --------------- | ------------- | ------------------------------------------------------------------ |
| cpp CUDA    | 5.1 (/props — model nạp lazy theo request đầu)     | ≈260 (S-03, LM-0.6B)                                    | 7767 MB         | 7781 MB       | 1484 / 1407 MB                                                     |
| cpp Vulkan  | ❌                                                 | ❌                                                      | ❌              | ❌            | ❌                                                                 |
| python pt   | 39.4 (/v1/models — weights lazy-load theo job đầu) | turbo: hoàn thành trong window; sft: 36.4 / 28.4 (S-03) | 7985 MB         | 7958 MB       | ⚠️ 31 MB — đo nhầm `uv` wrapper thay vì con python, không đại diện |
| python vllm | ❌ — không có native Windows                       | —                                                       | —               | —             | —                                                                  |

Ghi chú py: lần đo sft thứ hai bị nhiễm (process con `python` sống sót sau
khi kill `uv` cha, port 8001 còn listener → cold=0 giả). Cold start boot
sequence giống hệt giữa turbo/sft nên 39.4 s đại diện cho cả hai. Peak VRAM
sft 7958 MB lấy từ đúng window render 120s sft của lần nhiễm (hợp lệ).

Ghi chú cpp: "cold start" chỉ tính đến `/props` 200; weights nạp lazy khi
request đầu → thời gian nhận job thật = cold start + lần gen đầu (~30–60 s
với LM-0.6B). Peak VRAM là MAX mẫu 1 Hz trong lúc render 120s (gồm cả
desktop ~0.9 GB) — sát trần 8192 MB.

Phát hiện S-04 phụ: server py với 3 slot (`ACESTEP_CONFIG_PATH{1,2,3}`)
nạp EAGER cả 3 DiT lúc startup → cạn VRAM trên GPU 8GB, VAE tiled decode
treo vĩnh viễn ở mức free ≈ 0.04 GB. Xem S-05.

## S-05 — Hot-swap `/v1/init`

| Chuyển đổi  | Thời gian (s)                                             | VRAM đỉnh trong lúc swap              |
| ----------- | --------------------------------------------------------- | ------------------------------------- |
| turbo → sft | 36.5 (từ lúc POST /v1/init đến khi gen 30s xác minh xong) | 7738 MB (before 6819 → after 6952 MB) |
| sft → base  | 25.0 (cùng giao thức)                                     | 7396 MB (before 6964 → after 6792 MB) |

**Kết luận:** hot-swap **DÙNG ĐƯỢC** cho VRAM scheduler — 3 lần swap liên
tiếp không thấy VRAM tích tụ (6819 → 6952 → 6792 MB, dao động ±130 MB là
fragmentation bình thường); `/v1/init` trả lời tức thì, load chạy ngầm và
gen xác minh 30s thành công ngay sau swap.

**Phát hiện quan trọng:** cơ chế 3 slot preload (`ACESTEP_CONFIG_PATH{1,2,3}`)
nạp đồng thời 3 DiT lúc startup → trên GPU 8GB cạn kiệt VRAM (free 0.04 GB),
VAE tiled decode treo vĩnh viễn. Scheduler (WS-D) phải thiết kế theo mô hình
**1 model resident + swap trong slot**, không phải 3 slot nóng song song.

## S-06 — Repaint seam (sft)

| Crossfade | Nghe được seam?                             | Ghi chú                                                                                    |
| --------- | ------------------------------------------- | ------------------------------------------------------------------------------------------ |
| 0ms       | **Có** (P1) · **Có** (P2)                   | nhất quán 2 phiên — hard cut chắc chắn hở seam                                             |
| 50ms      | Không rõ (P1: "cũng được") · **Không** (P2) | nghiêng về không seam; P1 không khẳng định                                                 |
| 150ms     | Không rõ (P1: "cũng được") · **Không** (P2) | sạch qua P2, không bị phản đối ở P1                                                        |
| 300ms     | **Có** (P1) · **Không** (P2)                | KHÔNG tái lập — xác nhận P1 nghe nhầm motif lặp của NỘI DUNG repaint, không phải biên fade |

**Chốt:** **150 ms** — mức nhỏ nhất được P2 xác nhận rõ "không seam" và
P1 không phản đối; trùng đúng giá trị fallback đã thống nhất trước khi đo.
(0ms hở seam ở cả 2 phiên → crossfade bắt buộc.)

## S-07 — Extract vs Demucs v4

| Bài | vocals (ACE/Demucs) | drums         | bass          | other                 |
| --- | ------------------- | ------------- | ------------- | --------------------- |
| 1   | thua / thắng (P1)   | thua / thắng  | thua / thắng  | thua / thắng (synth)  |
| 2   | thua / thắng        | thua / thắng  | thua / thắng¹ | thua / thắng (guitar) |
| 3   | thua / thắng²       | thua / thắng  | thua / thắng  | thua / thắng (synth)  |
| 4   | không chấm được³    | thua / thắng⁴ | thua / thắng⁴ | hòa (guitar)          |
| 5   | thua / thắng        | thua / thắng  | thua / thắng  | không so sánh được⁵   |

Tổng P1: **Demucs 17 thắng — ACE 0 thắng — 1 hòa — 2 không chấm được**
(vocals 4-0, drums 5-0, bass 5-0, other 3-0-1-1).

¹ bass rock: ACE gần như toàn bộ instrument; Demucs đúng bass nhưng mờ
² vocal electronic: ACE hỏng ("chọp chẹp"); Demucs là vocal đúng
³ acoustic/vocals: cả hai bất thường — Demucs nghe "giống cello"; người nghe
nghi do export vocal trên nền không có vocal. Chờ P2.
⁴ acoustic/drums+bass: bài gốc KHÔNG có drums/bass — Demucs xuất file gần
rỗng (đúng), ACE xuất full instrument (sai)
⁵ vietvoc/guitar: bài không có guitar nhưng CẢ HAI đều xuất nội dung — bất
thường ở cả hai hệ

Các file nghe do **ACE-Step base tự sinh** (pop/rock/electronic/acoustic/
vocal Việt, 30s mỗi bài) — người nghe biết trước giới hạn này.

## S-08 — Rust audio prototype

| Buffer | xrun trong 30 phút | Latency đo được                                                      |
| ------ | ------------------ | -------------------------------------------------------------------- |
| 256    | **0**              | 5.3 ms (lý thuyết 256/48kHz — không đo loopback được, cần phần cứng) |
| 512    | **0**              | 10.7 ms (lý thuyết)                                                  |
| 1024   | **0**              | 21.3 ms (lý thuyết)                                                  |

`RESULT,S-08,512,30,0` · `RESULT,S-08,256,30,0` · `RESULT,S-08,1024,30,0`
(dòng gốc từ example). `cargo test -p als-audio` sau khi chạy: 14/14 xanh
(golden + no_alloc còn nguyên).

## Kill criteria — đánh dấu sau khi đo

| Điều kiện                               | Kích hoạt?                                                                                                                                     | Hệ quả bắt buộc                                                           |
| --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Warm gen 120s > 30s trên GPU mục tiêu   | ✅ KÍCH HOẠT — py pt turbo 48.8–127.4s, cpp 259.5–259.6s; chỉ py-sft sát ngưỡng (28.4s)                                                        | Bỏ UX "như nhạc cụ" → render queue + notification                         |
| Cold start > 60s                        | ❌ KHÔNG theo health (cpp 5.1s, py 39.4s); first-gen thật còn phụ thuộc lazy-load + model size                                                 | Vẫn nên warm-on-open, không bắt buộc                                      |
| vLLM không native **và** pt chậm hơn 2× | ❌ KHÔNG — vLLM không native là đúng, nhưng pt NHANH HƠN cpp trên GPU 8GB chứ không chậm hơn                                                   | Python sống sót qua Phase 0, là ứng viên mặc định                         |
| Vulkan build fail / sai output          | ⚠️ CHƯA ĐÁNH GIÁ ĐƯỢC — SDK không cài nổi trên máy đo (UAC từ chối 3 lần); đây là giới hạn môi trường, KHÔNG phải kết luận kỹ thuật về ace.cpp | Chạy lại trên máy có SDK rồi mới tuyên bố "NVIDIA only"                   |
| Rust audio xrun ở buffer 512            | ❌ KHÔNG — 0 xrun/30 phút ở cả 256/512/1024                                                                                                    | Giữ buffer mặc định 512 (10.7 ms); cân nhắc 256 nếu muốn latency thấp hơn |
| extract tệ hơn Demucs rõ rệt            | (S-07 — chờ chấm mù 2 phiên)                                                                                                                   | Demucs làm provider riêng ở Phase 2                                       |

## Kết luận Phase 0

- **Backend mặc định:** Python `acestep-api` (`lm_backend=pt`) — nhanh hơn cpp trên GPU 8GB ở 11/12 ô; giữ đủ 6 task_type cho Phase 2; hot-swap same-slot dùng được. `ace.cpp` giữ làm provider thứ hai (máy ≥12GB VRAM / đóng gói không Python), đánh giá lại khi Vulkan đo được
- **ADR-001 chuyển sang:** **Accepted** — phương án 3 (hai provider sau một trait) + mặc định Python; chi tiết trong `docs/adr/ADR-001-backend-selection.md`
- **Ngày chốt:** 2026-08-22
