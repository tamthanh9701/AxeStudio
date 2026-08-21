# AGENTS.md — Quy tắc thi công AxeStudio

Đọc hết file này trước khi sửa dòng code đầu tiên. Áp dụng cho cả người lẫn AI coding agent.

Các mục đánh dấu **CẤM** là điều kiện chặn merge, không phải gợi ý phong cách.

---

## 1. Ba luật không được phá

1. **Contract trước, code sau.** Schema SQLite, `trait RenderProvider`, và bề mặt IPC nằm trong `docs/contracts/`. Muốn đổi contract → mở PR riêng chỉ chứa thay đổi contract + ADR, không kèm code implement.
2. **Một PR = một ticket.** Không “tranh thủ dọn luôn”. Refactor ngoài phạm vi → PR riêng.
3. **Ngân sách hiệu năng là điều kiện merge.** Xem `docs/perf-budget.md`. CI bench fail → không merge, kể cả khi test chức năng xanh.

---

## 2. Ranh giới crate

Bảng này là bắt buộc. Thêm dependency nội bộ ngoài bảng → từ chối PR.

| Crate              | Được phụ thuộc vào                                      | Vai trò                                   |
| ------------------ | ------------------------------------------------------- | ----------------------------------------- |
| `als-core`         | — (không crate nội bộ nào)                              | kiểu chung, id, error, canonicalize, hash |
| `als-audio`        | **— (không gì cả, kể cả `als-core`)**                   | audio engine realtime                     |
| `als-project`      | `als-core`                                              | SQLite, migration, manifest               |
| `als-assets`       | `als-core`                                              | content-addressed store, peaks            |
| `als-media`        | `als-core`                                              | decode, resample, loudness                |
| `als-provider`     | `als-core`                                              | trait + contract tests + mock             |
| `als-provider-cpp` | `als-core`, `als-provider`                              | client `ace-server`                       |
| `als-provider-py`  | `als-core`, `als-provider`                              | client `acestep-api`                      |
| `als-orchestrator` | `als-core`, `als-provider`, `als-project`, `als-assets` | queue, cache, scheduler                   |
| `als-desktop`      | tất cả                                                  | shell Tauri, đăng ký command              |

**Tại sao `als-audio` không được phụ thuộc `als-core`:** một khi nó nhìn thấy `serde`, `String`, `HashMap` của domain, sớm hay muộn cũng có người allocate trong callback. Cắt phụ thuộc là cách rẻ nhất để lỗi đó không xảy ra. Giao tiếp với phần còn lại qua kiểu nguyên thuỷ (`u64`, `f32`, `[f32]`) và struct định nghĩa nội bộ.

---

## 3. `als-audio` — luật realtime

Audio callback chạy ở thread ưu tiên cao. Bất kỳ thao tác nào có thể block đều gây xrun nghe thấy được.

### CẤM trong phạm vi audio callback

- **CẤM** cấp phát: `Vec::new`, `Vec::push`, `Box::new`, `String`, `format!`, `collect()`, `to_vec()`, `clone()` trên kiểu có heap.
- **CẤM** khoá: `Mutex`, `RwLock`, `Condvar`, `channel` có block. Chỉ dùng SPSC lock-free (`rtrb`) và `Atomic*`.
- **CẤM** I/O: đọc file, log, `println!`, `dbg!`, `tracing::info!`.
- **CẤM** `panic!`, `unwrap()`, `expect()`, indexing có thể out-of-bounds. Underrun → trả silence + tăng counter.
- **CẤM** `std::thread::sleep`, spin-wait, syscall.

Mọi hàm chạy trong callback phải đánh dấu `#[rt_safe]` (attribute tài liệu trong `als-audio/src/rt_guard.rs`) và nằm dưới `assert_no_alloc` ở build debug.

### Bắt buộc

- Lệnh từ control thread đi qua **một** SPSC queue duy nhất.
- Trạng thái đọc ngược về UI (playhead, meter) đi qua `Atomic*`, **không qua event IPC**.
- Mọi thay đổi mixer phải có golden buffer test kèm theo.

---

## 4. Bindings TypeScript

`packages/bindings/src/generated.ts` **sinh tự động**.

- **CẤM** sửa tay file này.
- **CẤM** khai báo lại kiểu domain ở phía TS (`interface Clip {...}` viết tay → từ chối PR).
- Đổi struct Rust → chạy `pnpm bindings:generate` và commit kèm trong cùng PR.
- CI chạy `pnpm bindings:check`; lệch → fail.

---

## 5. Hash và cache

Hai tầng cache là lý do app này dùng được. Đừng phá chúng.

- `plan_hash = blake3(canonicalize(recipe **trừ** trường sampling))` — kết quả LM.
- `render_hash = blake3(plan_hash + sampling + model_checksum + PIPELINE_VERSION)` — kết quả DiT.

**CẤM**:

- **CẤM** dùng `serde_json::to_string` làm đầu vào hash — không đảm bảo thứ tự key.
- **CẤM** để `HashMap` chưa sort lọt vào `canonicalize`.
- **CẤM** đặt trường riêng của ACE-Step (ví dụ `lm_backend`, `shift`) vào `GenerationRecipe`. Recipe phải trung lập với provider; tham số riêng nằm ở `ProviderOverrides`.
- Đổi thuật toán canonicalize → **bắt buộc** tăng `PIPELINE_VERSION`.

Tiếng Việt có dấu phải chuẩn hoá **NFC** trước khi hash. Đây không phải chi tiết nhỏ: cop-paste từ macOS ra NFD, gõ trên Windows ra NFC, cùng một câu lời sẽ trượt cache nếu không chuẩn hoá.

---

## 6. Provider

- Mọi provider phải pass **cùng một** bộ test ở `crates/als-provider/tests/contract.rs`.
- Thêm provider mới → thêm vào danh sách chạy contract test, không viết bộ test riêng.
- `cancel()` phải trả `CancelOutcome::TooLate` khi job đã dispatch. **CẤM** giả vờ huỷ thành công.
- **CẤM** gọi HTTP trực tiếp từ `als-orchestrator` — luôn qua trait.

---

## 7. UI

- **CẤM** gọi API bên ngoài trực tiếp từ component. Mọi thứ qua `src/ipc/`.
- **CẤM** hardcode danh sách model / capability trong component — đọc từ `engine_status`.
- **CẤM** hiện control mà kéo không đổi gì. Tham số không áp dụng cho model/task hiện tại thì ẩn hoặc disable kèm lý do.
- Playhead đọc từ shared atomic trong `requestAnimationFrame`. **CẤM** bắn event 60 lần/giây qua IPC.

---

## 8. Định dạng ticket

Mọi ticket giao cho agent phải có đủ 5 phần. Thiếu phần nào thì đừng bắt đầu — hỏi lại.

1. **Bối cảnh** — tại sao cần, ràng buộc xung quanh.
2. **File được đụng** — danh sách đóng. Ngoài danh sách → hỏi trước.
3. **Yêu cầu** — đánh số.
4. **Acceptance** — đo được bằng test hoặc bằng số. “Hoạt động tốt” không phải acceptance.
5. **CẤM** — danh sách việc không được làm.

Xem mẫu ở `.github/ISSUE_TEMPLATE/task.md`.

---

## 9. Definition of Done

Một PR chỉ xong khi đủ cả sáu:

- [ ] `cargo fmt --check` và `cargo clippy -- -D warnings` sạch
- [ ] Test mới phủ đúng acceptance của ticket, không phải test phụ trợ
- [ ] Không chạm file ngoài danh sách “File được đụng”
- [ ] Bindings đã regenerate nếu đổi kiểu Rust
- [ ] Không vượt ngân sách ở `docs/perf-budget.md`
- [ ] Nếu đổi quyết định kiến trúc → có ADR mới trong `docs/adr/`

---

## 10. Khi không chắc

Dừng lại và hỏi. Cụ thể là khi:

- Ticket mâu thuẫn với contract trong `docs/contracts/`.
- Cần thêm dependency mới.
- Cần đụng file ngoài danh sách.
- Acceptance không đo được bằng test.

Đoán rồi viết 500 dòng sai tốn hơn hỏi một câu.
