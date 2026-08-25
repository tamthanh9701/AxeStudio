-- 002: bỏ UNIQUE(render_hash) trên take — BUG #3 (issue #14).
--
-- Cache-hit tầng 2 phải tạo ROW TAKE RIÊNG cho clip mới (trỏ CÙNG
-- asset_id — audio không bị copy), nếu không takes_for_clip(clip mới)
-- mãi rỗng và take không bao giờ hiện trong rack. UNIQUE(render_hash)
-- chặn row thứ hai cùng hash.
--
-- render_hash vẫn là khoá tra cứu tier-2: take_by_render_hash lấy một
-- row bất kỳ — các row cùng hash có nội dung audio như nhau.
DROP INDEX IF EXISTS idx_take_render;
