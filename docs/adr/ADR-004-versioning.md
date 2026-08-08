# ADR-004 — Versioning và migration schema project

- **Trạng thái:** Accepted

## Quyết định

Phiên bản app theo `MAJOR.MINOR.PATCH`.

- **Tăng MAJOR khi đổi project schema** (bảng SQLite, layout `.aiproj`, format manifest). Bản app cũ không mở được project của bản mới hơn. Phải kèm migration đi lên; không có migration đi xuống.
- **MINOR:** tính năng mới, không đổi schema.
- **PATCH:** sửa lỗi.

## Ràng buộc

- Migration chỉ đi một chiều lên. Mở project của app mới hơn → từ chối mở, hiện thông báo yêu cầu cập nhật. **Cấm** tự ý downgrade.
- Mọi migration phải có test up (từ version n-1) và test dữ liệu giữ nguyên.
- `schema_version` là một số nguyên duy nhất, tăng đơn điệu, không nhảy cóc.

## Lý do không dùng schema tự do

Project file là thứ người dùng quý nhất. Một lần mở project rồi save hỏng là mất niềm tin vĩnh viễn. Schema đóng + migration một chiều là cách duy nhất để test hết được.
