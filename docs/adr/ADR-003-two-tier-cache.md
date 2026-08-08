# ADR-003 — Cache hai tầng theo `plan_hash` / `render_hash`

- **Trạng thái:** Accepted

## Bối cảnh

Pipeline ACE-Step 1.5 có hai pha tách bạch:

```
prompt + lyrics ─► [ LM ] ─► audio_codes ─► [ DiT + VAE ] ─► WAV
                 (Qwen3)                      (8 hoặc 50 steps)
```

Pha LM chỉ phụ thuộc nội dung sáng tác. Pha DiT phụ thuộc thêm tham số sampling (`seed`, `inference_steps`, `guidance_scale`, `shift`, `infer_method`).

Hành vi phổ biến nhất của người dùng là **đổi seed để nghe bản khác** — giữ nguyên mọi thứ còn lại. Nếu mỗi lần như vậy đều chạy lại LM thì ta đang đốt thời gian cho một kết quả giống hệt lần trước.

## Quyết định

Hai bảng cache, hai khóa:

```
plan_hash   = blake3( canonicalize(recipe \ sampling) )
render_hash = blake3( plan_hash + sampling + model_checksum + PIPELINE_VERSION )
```

Quy trình tra:

1. Tra `render_hash` → trúng thì trả asset ngay, không gọi worker.
2. Trượt → tra `plan_hash` → trúng thì bỏ qua pha LM, chỉ gọi `render()` với `audio_codes` có sẵn.
3. Trượt cả hai → chạy đủ `plan()` rồi `render()`.

### `canonicalize` — định nghĩa chính xác

Thứ tự các bước là một phần của contract:

1. Chuẩn hoá Unicode **NFC**.
2. `trim()` hai đầu; gom mọi chuỗi khoảng trắng liên tiếp thành một dấu cách; chuẩn hoá newline về `\n`.
3. Sắp xếp khóa object theo thứ tự byte.
4. Làm tròn số thực về 4 chữ số thập phân, in không dùng ký hiệu mũ.
5. Bỏ trường có giá trị rỗng hoặc null.

## Hệ quả

- `PIPELINE_VERSION` là hằng số trong `als-core`. **Bất kỳ** thay đổi nào ở `canonicalize` hoặc ở hậu kỳ audio đều phải tăng số này. Quên tăng = trả cho người dùng file cũ với tham số mới, một lỗi gần như không debug được.
- `model_checksum` nằm trong `render_hash` để đổi quant (Q8_0 → Q4_K_M) không trả nhầm cache cũ.
- Tiếng Việt có dấu bắt buộc NFC — cùng một câu lời copy từ macOS (NFD) và gõ trên Windows (NFC) phải ra cùng hash. Có property test cho việc này.
- `ProviderOverrides` **không** tham gia `plan_hash` nhưng **có** tham gia `render_hash`.
