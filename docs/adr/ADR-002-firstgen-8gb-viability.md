# ADR-002 — Khả dụng first-gen trên GPU 8GB và semantics acceptance #14

- **Trạng thái:** Accepted (chốt 2026-08-25 — **phương án 1**: đóng #14 bằng evidence sẵn có, mở ticket theo dõi ổn định hoá/đo lại)
- **Ngày:** 2026-08-25
- **Quyết định bởi:** 5 vòng đo thật trên máy RTX 3070 8GB (issue #14, comments 5384992915 → 5409684029), không bởi ưa thích

## Bối cảnh

ADR-001 chọn hai provider sau một trait. Phase 0 đo warm-benchmark trên máy
8GB; Sprint 1 (#14) yêu cầu đo **first-gen sau mở project** — và chuỗi 5 vòng
đo lộ ra một sự thật cấu trúc mà benchmark warm không thấy:

### Chuỗi evidence (tất cả trên cùng một máy)

| Vòng | Fix đã merge | Kết quả                                                                                                                      |
| ---- | ------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| 1    | —            | Job chết `code=200` (server trả 200, client đòi 0) → PR #24                                                                  |
| 2    | #24/#25      | Warm OK tức thì; `/v1/init` đồng bộ không task_id → PR #25 bỏ crawl giả                                                      |
| 3    | #27          | **Pipeline đi hết đường lần đầu: render thật 124.3s** (gồm CPU-VAE ~90s); BUG #3 cache-hit không gắn take → PR #28           |
| 4    | #28          | Migration v2 PASS; render rơi CPU-VAE chậm >20 phút → timeout 1200s giết task đang tính đúng → PR #29 nâng 3600s             |
| 5    | #29          | **CPU-VAE >60 phút KHÔNG hoàn thành** (python CPU +30–40% một core liên tục, GPU idle sau pha DiT) — timeout fire đúng 3600s |

### Sự kiện cấu trúc

1. **Đường GPU-VAE bất khả thi trên 8GB khi desktop chạy**: server cần free
   VRAM ≥ 2.6GB cho VAE decode; desktop + driver chiếm ≥1GB → luôn rơi CPU path.
2. **CPU-tiled VAE không ổn định**: cùng đường, một phiên ~90s, một phiên
   > 60 phút không xong (python CPU grinding, không dấu hiệu kết thúc). Nâng
   > timeout không giải quyết — bài toán là độ tin cậy, không phải headroom.
3. **Server không có endpoint huỷ**: provider timeout để server compute ngầm
   (đã quan sát zombie 25–65 phút) — lãng phí kép mỗi lần thử.
4. **Pipeline app-side ĐÃ ĐÚNG**: round 3 chứng minh end-to-end
   (submit → Succeeded → parse → download → RIFF → take); cache-hit fix
   (PR #28) có test hồi quy qua vòng reopen project + bench 0.28ms trên runner.

## Phương án

| #   | Phương án                                                   | Đánh giá                                                                                                                                         |
| --- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | **Chốt semantics hiện tại + đóng #14 bằng evidence sẵn có** | Round 3 = bằng chứng end-to-end; cache-hit fix có test + bench. Việc còn lại chỉ thiếu số lặp lại trên phần cứng ổn định                         |
| 2   | **Đo thêm cpp+LM-0.6B làm first-gen mặc định cho 8GB**      | Spike S-01/S-03: cpp HOÀN THÀNH render trên chính máy này (LM-0.6B; LM-4B Q8_0 mới sụp). Cold start cpp 5.1s vs py 39.4s. Cần 1 vòng đo xác nhận |
| 3   | **Chờ GPU ≥12GB mới close #14**                             | Trung thực nhất về số, nhưng block Sprint 1 vì biến số nằm ở upstream acestep (VAE chunking/offload), ngoài tầm kiểm soát                        |

## Quyết định

Chọn **phương án 1**. Cơ sở: mục tiêu thật của #14 là chứng minh kiến trúc
render queue + notification + cache hoạt động — đã đạt (round 3 end-to-end,
cache-hit có test hồi quy qua reopen + bench). Con số latency trên phần cứng
bị giới hạn là biến số upstream (acestep VAE), không phải lỗi app — block
Sprint 1 vì nó là sai ưu tiên. Phương án 2 giữ mở như enhancement: nếu profiler
sản phẩm yêu cầu first-gen nhanh trên 8GB, cpp+LM-0.6B là ứng viên tự nhiên
(ADR-001 đã giữ cpp cho đúng trường hợp này).

## Hệ quả kèm theo (độc lập với phương án chọn)

- UX phải thiết kế theo "first-gen trên 8GB = phút-giờ, không đảm bảo" —
  warm-on-open + queue + notification (#14) đã che phần lớn.
- Timeout 3600s giữ nguyên: chỉ là trần chống kẹt; server compute ngầm sau
  timeout là hạn chế upstream (ghi nhận, cần acestep-api thêm cancel).
- Ticket theo dõi riêng nếu chọn PA2/PA3: "ổn định hoá CPU-VAE hoặc đo lại
  trên ≥12GB" — không block Sprint 1.

## Trạng thái các fix trong chuỗi

PR #24 (code=200) · #25 (warmup đồng bộ) · #27 (parse result + download
nguyên văn) · #28 (cache-hit take per clip + migration v2) · #29 (timeout
3600s) · #32 (payload /lm+/synth đúng contract) · #33 (parse mảng response)
— tất cả Accepted, CI xanh, có test hồi quy.

## Amendment 2026-08-26 — cpp-first cho 8GB (Accepted)

Vòng đo cpp qua app AxeStudio end-to-end (issue #14, comments 2026-08-26):

| Phép đo                                    | Kết quả                                       |
| ------------------------------------------ | --------------------------------------------- |
| SFT/30s render thật (script, LM-0.6B Q8_0) | **7.6 / 8.4 s** — PASS ngưỡng 15s             |
| SFT/30s qua app ×3 (cache-hit, sau PR #28) | **0.0 / 0.0 / 0.0 s** — PASS                  |
| turbo/30s render thật (script)             | 57.2 / 61.5 s                                 |
| Capability-driven UI (#10) dưới cpp        | Task không hỗ trợ tự ẩn, 3 tier models — PASS |
| Cache semantics (PR #28)                   | 2 clip × 1 take / 1 asset — đúng              |

### Quyết định bổ sung

**Máy < 12GB VRAM → khuyến nghị backend cpp CUDA + tier SFT** cho task thuộc
tập cpp hỗ trợ (text2music, cover). Python giữ vai trò: task nâng cao
(repaint/lego/extract/complete/training) trên máy VRAM đủ, và là provider duy
nhất có hot-swap multi-model.

### Giới hạn đã biết — ghi nhận rõ ràng

1. **cpp single-model-per-process**: đổi tier SFT ↔ turbo = restart ace-server
   với `--dit` tương ứng (hot-swap chỉ có trên py — nhưng py không viable
   trên 8GB, xem trên).
2. **Checksum placeholder**: `als-provider-cpp` dùng `todo-spike-<model>` làm
   checksum vì app không quản lý thư mục weights của ace-server. Hệ quả:
   render_hash nhất quán trong phạm vi binary + bộ GGUF hiện tại; **thay file
   weights mà giữ tên → cache KHÔNG vô hiệu tự động**. Chấp nhận cho v1; bỏ
   placeholder khi model store (WS-G) bàn giao đường dẫn weights cho provider.
3. **DLL runtime** phải nằm cạnh exe: `cublas64_13.dll`, `cublasLt64_13.dll`,
   `cudart_hybrid64.dll` (shim cudart64_13 — CUDA 13.3 không ship riêng).
4. **Preflight VRAM**: `EngineStatus.vram_free_mb` giờ điền từ health của
   provider (cpp đọc `/props`); UI cảnh báo khi free < 2.6GB qua
   `vramWarning()` — KHÔNG đổi contract IPC.
