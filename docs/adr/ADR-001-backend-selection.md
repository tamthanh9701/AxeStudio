# ADR-001 — Chọn backend suy luận mặc định

- **Trạng thái:** Accepted (Phase 0 chốt ngày 2026-08-22)
- **Ngày:** 2026-08-22
- **Quyết định bởi:** số liệu trong `docs/phase0/spike-report.md`, không bởi ưa thích

## Bối cảnh

ACE-Step 1.5 có hai đường chạy trên Windows, và chúng khác nhau về bản chất chứ không chỉ khác về hiệu năng.

### A. `acestep.cpp` — GGML/GGUF

Binary: `ace-lm`, `ace-synth`, `ace-server`, `ace-understand`, `neural-codec`. Server HTTP ở `GET /props`, `POST /lm`, `POST /synth`, `POST /understand`.

- Không cần Python runtime — người dùng cuối không phải cài gì.
- Quant Q4_K_M → Q8_0 → BF16; bộ Q8_0 tối thiểu ≈ 7.7GB.
- Backend CUDA / HIP / Vulkan — Vulkan mở đường cho AMD và Intel.
- Đóng gói trực tiếp vào `.msi` được.
- **Nhược:** chỉ có một phần bề mặt tính năng. Không có `repaint`, `lego`, `extract`, `complete`, không có training.

### B. Python `acestep-api` — port 8001

- Đủ 6 `task_type`: `text2music`, `cover`, `repaint`, `lego`, `extract`, `complete`.
- Hot-swap model qua `POST /v1/init` (slot 1–3).
- `lm_backend` chọn `vllm` hoặc `pt`.
- LoRA training API.
- **Nhược:** kéo theo Python 3.11–3.12 + `uv` + vài GB wheel. Trải nghiệm cài đặt tệ hơn hẳn. vLLM native trên Windows **chưa được kiểm chứng** — lịch sử là Linux-first.
- Không có endpoint cancel, không có WebSocket/SSE. Tiến độ phải polling `POST /query_result`.

## Phương án

| #   | Phương án                      | Đánh giá                                                                    |
| --- | ------------------------------ | --------------------------------------------------------------------------- |
| 1   | Chỉ `acestep.cpp`              | Cài đặt gọn nhất. Mất repaint/extract → Phase 2 gần như không làm được      |
| 2   | Chỉ Python                     | Đủ tính năng. First-run khủng khiếp, khó ship cho người không rành kỹ thuật |
| 3   | **Hai provider sau một trait** | Phức tạp hơn ở orchestrator, nhưng giữ được cả hai đường                    |

## Quyết định

Chọn **phương án 3**: định nghĩa `trait RenderProvider` và hiện thực cả hai. `capabilities()` cho biết provider nào làm được gì; UI ẩn thứ không làm được.

Backend mặc định khi cài đặt đã chốt bằng số đo Phase 0 — xem "Quyết định
mặc định" bên dưới.

### Tiêu chí chốt — kết quả Phase 0 (máy đo: RTX 3070 8GB, driver 596.49, Win11 build 26200)

| Điều kiện quan sát được                        | Kết quả đo (LM-0.6B công bằng cả hai bên)      |
| ---------------------------------------------- | -------------------------------------------- |
| `cpp` chậm hơn `python` ≤ 30%                  | ❌ KHÔNG — cpp chậm 1.4× (sft/30) đến 14× (turbo/240); VAE decode của ggml ~1.7s/s audio là floor |
| `cpp` chậm hơn > 2×                            | ✅ ĐÚNG ở 11/12 ô ma trận (ngoại lệ duy nhất sft/30s: 8.4s vs 18.4s) |
| vLLM không chạy native **và** `pt` chậm hơn 2× | vLLM không native ✅ (Triton), nhưng `pt` NHANH HƠN cpp chứ không chậm hơn → điều kiện kép KHÔNG xảy ra |
| Vulkan build fail hoặc sai output              | ⚠️ Không đánh giá được trên máy đo (không cài nổi SDK — giới hạn môi trường, xem spike-report S-01) |

## Quyết định mặc định (Phase 0)

**Backend mặc định khi cài đặt: Python `acestep-api` (`lm_backend=pt`).**

Căn cứ:
1. Trên GPU 8GB, python-pt nhanh hơn cpp ở 11/12 ô; cấu hình ship mặc định
   của ace.cpp (LM-4B Q8_0 ≈ 7.7GB weights) còn sụp đổ hẳn từ 120s trở lên
   do tràn VRAM (650–1846s cho job 120–240s).
2. Python giữ trọn 6 task_type (repaint/extract là tính năng Phase 2);
   cpp chỉ phục vụ text2music + understand.
3. Hot-swap `/v1/init` dùng được (25–37s/swap, không leak VRAM) — đủ cho
   VRAM scheduler, với điều kiện thiết kế **1 model resident + swap trong
   slot**; preload 3 slot đồng thời làm cạn 8GB VRAM và treo VAE (S-05).
4. Cold start py 39.4s tới health (<60s); warm-on-open vẫn nên làm vì
   first-gen thật phụ thuộc lazy-load.
5. Kill criterion "warm gen 120s > 30s" KÍCH HOẠT → Sprint 1 phải có
   render queue + notification, không đặt cược vào UX realtime.

`ace.cpp` giữ lại như provider thứ hai sau trait: hợp lý cho máy ≥12GB VRAM
và cho mục tiêu đóng gói `.msi` không cần Python — đánh giá lại sau khi
hạ tầng Vulkan đo được và/hoặc có quant nhỏ hơn cho bộ model.

## Hệ quả

- `als-orchestrator` **không được** biết provider nào đang chạy. Mọi thứ qua trait.
- `GenerationRecipe` phải trung lập với provider. Tham số riêng đi trong `ProviderOverrides` và **không** tham gia `plan_hash`.
- UI phải xử lý được trường hợp một task không khả dụng với backend hiện tại, kèm lý do đọc được.
- Cancel phải có ba kết cục chứ không phải hai: huỷ được, quá muộn, lỗi. Python không có endpoint cancel nên `TooLate` là trạng thái thường gặp, không phải ngoại lệ.
