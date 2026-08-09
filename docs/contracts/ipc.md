# Contract IPC — nguồn sự thật

Đây là contract giữa `apps/desktop` (React) và `src-tauri` (Rust). Mọi thay đổi ở đây đi kèm PR riêng chỉ chứa thay đổi contract + ADR nếu cần.

Kiểu dữ liệu tương ứng được định nghĩa tại `crates/als-core` và sinh sang TS ở `packages/bindings`. Tên field trong JSON dùng `snake_case`; tên tham số command phía JS dùng camelCase (Tauri tự chuyển).

## Commands

### Project

| Command | Input | Output | Ghi chú |
| --- | --- | --- | --- |
| `project_create` | `{ path, name }` | `ProjectSnapshot` | Tạo `.aiproj` mới, fail nếu path đã tồn tại |
| `project_open` | `{ path }` | `ProjectSnapshot` | Chạy migration nếu schema cũ hơn; từ chối nếu mới hơn app |
| `project_save_as` | `{ path }` | `ProjectSnapshot` | Copy project sang vị trí mới, chuyển active project |
| `project_apply_edit` | `EditCommand` | `EditOutcome { edit, snapshot }` | Snapshot MỚI trả kèm — UI không bao giờ lệch state. Đẩy vào undo stack; emit `project:dirty` |
| `project_undo` / `project_redo` | — | `{ label, snapshot }` | label rỗng khi stack hết |

### Asset

| Command | Input | Output | Ghi chú |
| --- | --- | --- | --- |
| `asset_import` | `{ paths: string[] }` | `AssetId[]` | Decode + normalize 48kHz f32; peaks sinh trong cùng lượt → event `peaks:ready` |
| `asset_get` | `{ asset_id }` | `AssetInfo` | Metadata (duration, sample rate, channels) — UI import đặt clip theo độ dài thật |
| `asset_peaks` | `{ asset_id, zoom_level }` | `PeakView { spp, pairs }` | zoom_level ∈ 0..3 tương ứng 256/1024/4096/16384 spp. MVP trả JSON; nhị phân hoá ở Phase 2 nếu profiler nói cần |
| `asset_delete` | `{ asset_id }` | `()` | Chỉ xoá khi không còn take/clip tham chiếu (`ASSET_IN_USE`) — chưa implement ở v1 |

### Generation

| Command | Input | Output | Ghi chú |
| --- | --- | --- | --- |
| `generate_submit` | `{ clip_id, recipe, priority? }` | `JobId` | Vào queue, trả ngay; không chờ kết quả |
| `job_cancel` | `{ job_id }` | `Cancelled` \| `TooLate` | `TooLate` khi job đã dispatch — UI phải nói rõ |
| `take_list` | `{ clip_id }` | `TakeInfo[]` | Mới nhất trước |
| `take_promote` | `{ clip_id, take_id }` | `EditOutcome` | Đổi take active của clip (qua undo stack) |
| `take_star` | `{ take_id, starred }` | `()` | |
| `take_delete` | `{ take_id }` | `()` | Asset giữ lại nếu take khác tham chiếu |

### Transport

| Command | Input | Output |
| --- | --- | --- |
| `transport_play` | `()` | `()` |
| `transport_pause` | `()` | `()` |
| `transport_seek` | `{ position_ms }` | `()` |
| `transport_loop` | `{ start_ms, end_ms, enabled }` | `()` |
| `transport_position` | `()` | `{ frames, playing }` |

### Playhead — tại sao có `transport_position`

Contract gốc nói "playhead đọc từ `AtomicU64` chia sẻ". Điều đó đúng khi UI và engine chung một process. WebView2 của Tauri là **process khác** — không share memory được. Adaptation trung thực:

- UI **poll** `transport_position` trong `requestAnimationFrame` (request/response nhẹ, một consumer).
- **CẤM** bắn event playhead từ Rust — broadcast 60fps qua IPC làm giật cả UI lẫn audio.
- Meter cũng theo cơ chế poll này (gộp chung một call nếu cần).

### Engine

| Command | Input | Output |
| --- | --- | --- |
| `engine_status` | `()` | `EngineStatus` — backend, model warm, VRAM, queue depth |
| `engine_switch_backend` | `{ provider_id }` | `()` |
| `engine_warmup` | `{ model_id }` | `()` (no-op v1) |

### Export

| Command | Input | Output |
| --- | --- | --- |
| `export_render` | `ExportSpec` | `string` (out_path) | v1: **đồng bộ**, chỉ WAV 24-bit, sidecar `.meta.json` khi `include_metadata`. MP3/FLAC qua FFmpeg sidecar ở Sprint 6; khi đó chuyển thành job queue như plan gốc |

## Events (Rust → UI)

| Event | Payload | Khi nào |
| --- | --- | --- |
| `job:progress` | `{ job_id, percent, stage }` | Tối đa 4 lần/giây/job, throttle ở Rust |
| `job:state` | `{ job_id, state, error? }` | Mọi chuyển trạng thái |
| `take:ready` | `{ job_id, clip_id, take_id, cached }` | Take sẵn sàng gắn vào clip |
| `engine:status` | `EngineStatus` | Khi thay đổi, tối đa 1 lần/giây |
| `peaks:ready` | `{ asset_id }` | Peaks sinh xong |
| `project:dirty` | `{ dirty }` | Undo stack thay đổi |

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
