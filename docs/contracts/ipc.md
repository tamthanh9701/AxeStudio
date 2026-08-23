# Contract IPC — nguồn sự thật

Đây là contract giữa `apps/desktop` (React) và `src-tauri` (Rust). Mọi thay đổi ở đây đi kèm PR riêng chỉ chứa thay đổi contract + ADR nếu cần.

Kiểu dữ liệu tương ứng được định nghĩa tại `crates/als-core` và sinh sang TS ở `packages/bindings`. Tên field trong JSON dùng `snake_case`; tên tham số command phía JS dùng camelCase (Tauri tự chuyển).

## Commands

### Project

| Command                         | Input            | Output                           | Ghi chú                                                                                      |
| ------------------------------- | ---------------- | -------------------------------- | -------------------------------------------------------------------------------------------- |
| `project_create`                | `{ path, name }` | `ProjectSnapshot`                | Tạo `.aiproj` mới, fail nếu path đã tồn tại                                                  |
| `project_open`                  | `{ path }`       | `ProjectSnapshot`                | Chạy migration nếu schema cũ hơn; từ chối nếu mới hơn app                                    |
| `project_save_as`               | `{ path }`       | `ProjectSnapshot`                | Copy project sang vị trí mới, chuyển active project                                          |
| `project_apply_edit`            | `EditCommand`    | `EditOutcome { edit, snapshot }` | Snapshot MỚI trả kèm — UI không bao giờ lệch state. Đẩy vào undo stack; emit `project:dirty` |
| `project_undo` / `project_redo` | —                | `{ label, snapshot }`            | label rỗng khi stack hết                                                                     |

### Asset

| Command        | Input                      | Output                    | Ghi chú                                                                                                        |
| -------------- | -------------------------- | ------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `asset_import` | `{ paths: string[] }`      | `AssetId[]`               | Decode + normalize 48kHz f32; peaks sinh trong cùng lượt → event `peaks:ready`                                 |
| `asset_get`    | `{ asset_id }`             | `AssetInfo`               | Metadata (duration, sample rate, channels) — UI import đặt clip theo độ dài thật                               |
| `asset_peaks`  | `{ asset_id, zoom_level }` | `PeakView { spp, pairs }` | zoom_level ∈ 0..3 tương ứng 256/1024/4096/16384 spp. MVP trả JSON; nhị phân hoá ở Phase 2 nếu profiler nói cần |
| `asset_delete` | `{ asset_id }`             | `()`                      | Chỉ xoá khi không còn take/clip tham chiếu (`ASSET_IN_USE`) — chưa implement ở v1                              |

### Generation

| Command           | Input                            | Output                   | Ghi chú                                        |
| ----------------- | -------------------------------- | ------------------------ | ---------------------------------------------- |
| `generate_submit` | `{ clip_id, recipe, priority? }` | `JobId`                  | Vào queue, trả ngay; không chờ kết quả         |
| `job_cancel`      | `{ job_id }`                     | `Cancelled` \| `TooLate` | `TooLate` khi job đã dispatch — UI phải nói rõ |
| `take_list`       | `{ clip_id }`                    | `TakeInfo[]`             | Mới nhất trước                                 |
| `take_promote`    | `{ clip_id, take_id }`           | `EditOutcome`            | Đổi take active của clip (qua undo stack)      |
| `take_star`       | `{ take_id, starred }`           | `()`                     |                                                |
| `take_delete`     | `{ take_id }`                    | `()`                     | Asset giữ lại nếu take khác tham chiếu         |

### Transport

| Command              | Input                           | Output                |
| -------------------- | ------------------------------- | --------------------- |
| `transport_play`     | `()`                            | `()`                  |
| `transport_pause`    | `()`                            | `()`                  |
| `transport_seek`     | `{ position_ms }`               | `()`                  |
| `transport_loop`     | `{ start_ms, end_ms, enabled }` | `()`                  |
| `transport_position` | `()`                            | `{ frames, playing }` |

### Playhead — tại sao có `transport_position`

Contract gốc nói "playhead đọc từ `AtomicU64` chia sẻ". Điều đó đúng khi UI và engine chung một process. WebView2 của Tauri là **process khác** — không share memory được. Adaptation trung thực:

- UI **poll** `transport_position` trong `requestAnimationFrame` (request/response nhẹ, một consumer).
- **CẤM** bắn event playhead từ Rust — broadcast 60fps qua IPC làm giật cả UI lẫn audio.
- Meter cũng theo cơ chế poll này (gộp chung một call nếu cần).

### Engine

| Command                 | Input             | Output                                                                                                                                          |
| ----------------------- | ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `engine_switch_backend` | `{ provider_id }` | `()`                                                                                                                                            |
| `engine_warmup`         | —                 | Không có lệnh thủ công ở v1: model được nạp nóng **tự động** khi mở project (issue #14, ADR-001). Lệnh thủ công cân nhắc thêm khi có nhu cầu UI |

### Export

| Command         | Input        | Output              |
| --------------- | ------------ | ------------------- |
| `export_render` | `ExportSpec` | `string` (out_path) | v1: **đồng bộ**, chỉ WAV 24-bit, sidecar `.meta.json` khi `include_metadata`. MP3/FLAC qua FFmpeg sidecar ở Sprint 6; khi đó chuyển thành job queue như plan gốc |

## Events (Rust → UI)

| Event           | Payload                                | Khi nào                                |
| --------------- | -------------------------------------- | -------------------------------------- |
| `job:progress`  | `{ job_id, percent, stage }`           | Tối đa 4 lần/giây/job, throttle ở Rust |
| `job:state`     | `{ job_id, state, error? }`            | Mọi chuyển trạng thái                  |
| `take:ready`    | `{ job_id, clip_id, take_id, cached }` | Take sẵn sàng gắn vào clip             |
| `engine:status` | `EngineStatus`                         | Khi thay đổi, tối đa 1 lần/giây        |
| `peaks:ready`   | `{ asset_id }`                         | Peaks sinh xong                        |
| `project:dirty` | `{ dirty }`                            | Undo stack thay đổi                    |

### Warm-on-open (issue #14)

Khi mở project, orchestrator tự nạp nóng model mặc định (Turbo) **trước khi
người dùng kịp bấm generate** — kill criterion "warm gen 120s > 30s" của
Phase 0 loại bỏ UX realtime, warm giúp lượt generate ĐẦU không gánh thêm
cold-load. Warm đi qua CÙNG event `job:progress` / `job:state` như render,
với job_id pseudo có tiền tố **`warm:`** (`warm:<uuid>`):

- UI nhận biết job warm bằng tiền tố này (hiển thị nhãn riêng kiểu
  "Nạp model", không cho huỷ ở v1).
- Tiến độ là SỰ THẬT từ provider: py poll `/query_result` ≤ 4 lần/s khi
  `/v1/init` trả task handle; khi không có handle, percent ước lượng theo
  median đo được ở S-05 (25–37s) và DỪNG ở 95% — không bao giờ tự tuyên bố
  100% khi thiếu xác nhận "load xong" từ server.
- Warm và render KHÔNG BAO GIỜ chạy song song: acestep-api swap model trong
  slot sẽ làm hỏng job đang chạy (S-04/S-05). Warm khi slot bận → xếp hàng
  (`job:state = queued`) và chỉ chạy khi queue render cạn.
- Thất bại warm KHÔNG chặn mở project hay generate — gen đầu tiên chỉ
  chậm hơn (server lazy-load), UI hiện notice nhẹ.

## Playback wiring (v1)

- Mỗi track được **consolidate** thành một buffer timeline-absolute lúc: mở project, `project_apply_edit`, undo/redo, `take_promote`, event `take:ready` (`src-tauri/player.rs`).
- Clip **import** resolve audio trực tiếp từ `ClipSource::Imported.asset`; clip **generate** resolve qua `active_take` → asset.
- Engine rebuild mỗi lần refresh (click ~50ms, chấp nhận được ở v1). S2 thay bằng streaming per-clip + live swap (arc-swap).
- Consolidation bị chặn ở 5 phút/track để không bùng RAM với arrangement dài.

## Quy ước lỗi

Mọi command trả `Result<T, IpcError>`:

```json
{ "code": "PROVIDER_UNAVAILABLE", "message": "...", "retryable": false }
```

`code` là enum đóng trong `als-core::error`. UI map code → thông báo tiếng Việt, **không** hiển thị `message` thô cho người dùng cuối.
