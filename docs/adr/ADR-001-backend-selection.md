# ADR-001 — Chọn backend suy luận mặc định

- **Trạng thái:** Proposed
- **Ngày:** chưa — chốt vào cuối Phase 0
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

Cái **chưa chốt** là backend nào làm _mặc định khi cài đặt_. Đó là quyết định về trải nghiệm first-run, không phải về kỹ thuật.

### Tiêu chí chốt (điền từ Phase 0)

| Điều kiện quan sát được                        | Kết luận                                     |
| ---------------------------------------------- | -------------------------------------------- |
| `cpp` chậm hơn `python` ≤ 30%                  | Mặc định `cpp`. Python là tùy chọn nâng cao  |
| `cpp` chậm hơn > 2×                            | Mặc định Python, kèm installer tự lo runtime |
| vLLM không chạy native **và** `pt` chậm hơn 2× | Bỏ Python khỏi Phase 1 hoàn toàn             |
| Vulkan build fail hoặc sai output              | Tuyên bố "NVIDIA only" ở v1                  |

## Hệ quả

- `als-orchestrator` **không được** biết provider nào đang chạy. Mọi thứ qua trait.
- `GenerationRecipe` phải trung lập với provider. Tham số riêng đi trong `ProviderOverrides` và **không** tham gia `plan_hash`.
- UI phải xử lý được trường hợp một task không khả dụng với backend hiện tại, kèm lý do đọc được.
- Cancel phải có ba kết cục chứ không phải hai: huỷ được, quá muộn, lỗi. Python không có endpoint cancel nên `TooLate` là trạng thái thường gặp, không phải ngoại lệ.
