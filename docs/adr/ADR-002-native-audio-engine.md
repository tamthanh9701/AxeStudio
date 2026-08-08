# ADR-002 — Audio engine native Rust, không dùng Web Audio API

- **Trạng thái:** Accepted
- **Ngày:** Phase 0

## Bối cảnh

App dựng trên Tauri, nên lựa chọn hiển nhiên là dùng Web Audio API trong WebView. Nghiên cứu ban đầu cũng đề xuất như vậy.

Điều đó không đứng vững ở quy mô của một DAW.

## Lý do từ chối Web Audio

1. **Không kiểm soát được buffer size và thiết bị ra.** Không chọn được WASAPI exclusive, không đường nào tới ASIO. Người dùng nhạc cụ sẽ hỏi ASIO trong tuần đầu.
2. **GC của JS nằm trên đường dữ liệu.** Với 24 track streaming từ đĩa, một lần GC dừng là một lần nghe thấy.
3. **WebView trên Windows là WebView2 — không do ta kiểm soát phiên bản.** Một bản cập nhật Edge có thể đổi đặc tính timing của audio graph mà ta không biết trước.
4. **Streaming từ đĩa phải đi qua fetch + decode trong JS**, nhân đôi bộ nhớ và đặt decode vào đúng thread đang vẽ UI.
5. Playhead cần độ chính xác mức sample. Đồng bộ đồng hồ giữa audio graph và canvas qua JS là nguồn lỗi bất tận.

## Quyết định

Audio engine là crate Rust độc lập `als-audio`, chạy trong process host, ra thiết bị qua `cpal` → WASAPI shared ở v1.

WebView **chỉ** làm ba việc với audio: gửi lệnh transport, đọc playhead từ shared atomic, đọc meter từ shared atomic. Không có mẫu audio nào đi qua IPC.

## Hệ quả

- Phải tự viết mixer, transport, streaming reader, metering. Ước lượng 20 ngày công (WS-B).
- Đổi lại: `als-audio` test được hoàn toàn headless bằng golden buffer, không cần mở UI.
- ASIO mở được ở Phase 2 qua `asio-sys` mà không phải viết lại gì — chỉ thêm một backend `cpal`. Lưu ý license SDK của Steinberg phải xử lý riêng.
- `als-audio` **không** được phụ thuộc crate nội bộ nào, kể cả `als-core`. Xem `AGENTS.md` mục 2.
