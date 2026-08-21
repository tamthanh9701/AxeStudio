# Ngân sách hiệu năng

Đây là **điều kiện merge**, không phải mục tiêu tham khảo. CI bench fail → không merge, kể cả khi test chức năng xanh.

## Bảng ngân sách

| Chỉ số                           | Ngân sách                                    | Cách đo                                |
| -------------------------------- | -------------------------------------------- | -------------------------------------- |
| Timeline scroll/zoom             | ≥ 60fps, 24 track / 200 clip                 | Pixi ticker, script bench 10s liên tục |
| Audio callback                   | < 30% ngân sách @ buffer 512, 48kHz (≈3.5ms) | `als-audio` bench                      |
| Xrun                             | 0 trong 30 phút phát 8 track                 | counter trong engine                   |
| App cold start → UI sẵn sàng     | < 3s                                         | script đo từ spawn đến `ready` event   |
| Mở project 50 clip               | < 1.5s                                       | fixture project                        |
| Cache hit → nghe được            | < 200ms                                      | test orchestrator với mock             |
| Re-roll seed (plan hit)          | ≤ 40% thời gian full generate                | so thời gian 2 lần gọi                 |
| Peaks 3 phút audio               | < 800ms                                      | bench `als-assets`                     |
| RAM app idle (không tính worker) | < 600MB                                      | đo sau 60s idle                        |

## Quy tắc

1. Số trên đo trên máy target tối thiểu (8GB VRAM, 32GB RAM), không đo trên máy dev 64GB.
2. Bench chạy trên self-hosted runner có GPU thật — GitHub-hosted không có GPU.
3. Một PR được phép **nới** ngân sách chỉ khi kèm ADR giải thích tại sao.
4. Không "tối ưu sau". Nếu sprint hiện tại phá ngân sách thì sprint đó chưa xong.

## Vì sao khắt khe

App sinh nhạc có 3 đường realtime: audio callback (3.5ms mỗi vòng), UI 60fps (16.6ms mỗi frame), và tương tác generate (người dùng chờ). Cả ba đều fail _mềm_ — app không crash, chỉ trở nên khó chịu. Không gắn vào CI thì regression trôi qua review.
