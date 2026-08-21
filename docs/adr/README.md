# Architecture Decision Records

Mỗi quyết định kiến trúc có hậu quả lâu dài đều phải có một ADR. ADR **không bị xóa và không bị sửa nội dung** sau khi Accepted — muốn đổi thì viết ADR mới và đánh dấu cái cũ là Superseded.

| #                                     | Tiêu đề                                         | Trạng thái |
| ------------------------------------- | ----------------------------------------------- | ---------- |
| [001](ADR-001-backend-selection.md)   | Chọn backend suy luận mặc định                  | Proposed   |
| [002](ADR-002-native-audio-engine.md) | Audio engine native Rust, không dùng Web Audio  | Accepted   |
| [003](ADR-003-two-tier-cache.md)      | Cache hai tầng theo `plan_hash` / `render_hash` | Accepted   |
| [004](ADR-004-versioning.md)          | Versioning và migration schema project          | Accepted   |

Trạng thái hợp lệ: `Proposed` → `Accepted` → `Superseded by ADR-NNN` hoặc `Rejected`.
