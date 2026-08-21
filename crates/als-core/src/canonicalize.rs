//! Canonicalize — biến một JSON value thành CHUỖI BYTE DUY NHẤT.
//!
//! Đây là một phần của contract cache (ADR-003). BẤT KỲ thay đổi hành vi nào
//! ở file này đều phải tăng `PIPELINE_VERSION` trong `hash.rs`, nếu không sẽ
//! trả cho người dùng file cũ với tham số mới — lỗi gần như không debug được.
//!
//! Thứ tự bước (là một phần của contract):
//! 1. Chuỗi: NFC. Việc trim/gom whitespace là theo field, làm ở tầng recipe
//!    (`plan_view`), KHÔNG làm ở đây.
//! 2. Object: sort key theo thứ tự byte (serde_json::Map mặc định là BTreeMap),
//!    bỏ value null và chuỗi rỗng.
//! 3. Số thực: round 4 chữ số thập phân, in dạng thập phân thường (không mũ),
//!    -0.0 → 0. Số nguyên in nguyên — `7` và `7.0` ra cùng `"7"`.
//! 4. Array: giữ nguyên thứ tự và cả null (vị trí mang ngữ nghĩa).

use serde_json::Value;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum CanonicalizeError {
    #[error("số không hữu hạn (NaN/Infinity) không canonicalize được")]
    NonFiniteNumber,
}

/// Chuẩn hoá chuỗi MỘT DÒNG (prompt, key_scale, tag...):
/// NFC → gom mọi khoảng trắng (space, tab, newline, CR...) thành 1 space → trim.
pub fn normalize_line(s: &str) -> String {
    let nfc: String = s.nfc().collect();
    let mut out = String::with_capacity(nfc.len());
    let mut pending_space = false;
    for ch in nfc.chars() {
        if ch.is_whitespace() {
            // Chỉ đánh dấu, chưa ghi — để trim được cả đầu lẫn cuối.
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
            }
            out.push(ch);
            pending_space = false;
        }
    }
    out
}

/// Chuẩn hoá khối NHIỀU DÒNG (lyrics): giữ cấu trúc dòng vì newline mang ngữ
/// nghĩa cho LM ([Verse]/[Chorus]...). NFC → tách dòng → normalize từng dòng
/// → bỏ dòng trống → nối lại bằng `\n`.
pub fn normalize_block(s: &str) -> String {
    let nfc: String = s.nfc().collect();
    nfc.split('\n')
        .map(normalize_line)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Canonicalize một JSON value thành chuỗi byte duy nhất.
pub fn canonicalize(value: &Value) -> Result<String, CanonicalizeError> {
    let mut out = String::with_capacity(256);
    write_value(value, &mut out)?;
    Ok(out)
}

fn write_value(v: &Value, out: &mut String) -> Result<(), CanonicalizeError> {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            // Ưu tiên dạng nguyên để `7` và `7.0` hội tụ về cùng chuỗi.
            if let Some(u) = n.as_u64() {
                out.push_str(&u.to_string());
            } else if let Some(i) = n.as_i64() {
                out.push_str(&i.to_string());
            } else {
                write_f64(n.as_f64().ok_or(CanonicalizeError::NonFiniteNumber)?, out)?;
            }
        }
        Value::String(s) => {
            // NFC ở đây là lưới an toàn cuối; chuẩn hoá theo field đã xảy ra ở recipe.
            let nfc: String = s.nfc().collect();
            write_string(&nfc, out);
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            let mut first = true;
            // serde_json::Map (không bật preserve_order) là BTreeMap → đã sort.
            for (k, v) in map {
                let drop =
                    matches!(v, Value::Null) || matches!(v, Value::String(s) if s.is_empty());
                if drop {
                    continue;
                }
                if !first {
                    out.push(',');
                }
                first = false;
                write_string(k, out);
                out.push(':');
                write_value(v, out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn write_f64(f: f64, out: &mut String) -> Result<(), CanonicalizeError> {
    if !f.is_finite() {
        return Err(CanonicalizeError::NonFiniteNumber);
    }
    let mut r = (f * 10_000.0).round() / 10_000.0;
    if r == 0.0 {
        r = 0.0; // tắt -0.0
    }
    // Display của f64 không bao giờ dùng ký hiệu mũ.
    let mut s = format!("{r:.4}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    out.push_str(&s);
    Ok(())
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn drops_null_and_empty_object_fields() {
        let v = json!({"a": 1, "b": null, "c": "", "d": {"e": null}});
        assert_eq!(canonicalize(&v).unwrap(), r#"{"a":1,"d":{}}"#);
    }

    #[test]
    fn sorts_object_keys() {
        let v = json!({"b": 1, "a": 2, "c": 3});
        assert_eq!(canonicalize(&v).unwrap(), r#"{"a":2,"b":1,"c":3}"#);
    }

    #[test]
    fn rounds_floats_to_4_decimals() {
        let v = json!({"x": 0.8500001});
        assert_eq!(canonicalize(&v).unwrap(), r#"{"x":0.85}"#);
    }

    #[test]
    fn integer_and_float_forms_unify() {
        assert_eq!(
            canonicalize(&json!(7)).unwrap(),
            canonicalize(&json!(7.0)).unwrap()
        );
    }

    #[test]
    fn negative_zero_becomes_zero() {
        assert_eq!(canonicalize(&json!(-0.0)).unwrap(), "0");
    }

    #[test]
    fn nfc_normalization_in_strings() {
        // "Tiếng" dạng NFC vs NFD phải ra cùng bytes.
        let nfc = json!("Ti\u{1EBF}ng");
        let nfd = json!("Tie\u{0302}\u{0301}ng");
        assert_eq!(canonicalize(&nfc).unwrap(), canonicalize(&nfd).unwrap());
    }

    #[test]
    fn arrays_keep_nulls_and_order() {
        let v = json!([1, null, 2]);
        assert_eq!(canonicalize(&v).unwrap(), "[1,null,2]");
    }

    #[test]
    fn canonicalize_is_idempotent() {
        let v = json!({"z": [1, 2.5, "x"], "a": {"b": null, "c": 0.1}});
        let once = canonicalize(&v).unwrap();
        let parsed: Value = serde_json::from_str(&once).unwrap();
        assert_eq!(once, canonicalize(&parsed).unwrap());
    }

    #[test]
    fn rejects_non_finite() {
        assert_eq!(
            write_f64(f64::NAN, &mut String::new()),
            Err(CanonicalizeError::NonFiniteNumber)
        );
        assert_eq!(
            write_f64(f64::INFINITY, &mut String::new()),
            Err(CanonicalizeError::NonFiniteNumber)
        );
    }

    #[test]
    fn normalize_line_collapses_whitespace() {
        assert_eq!(
            normalize_line("  epic \t cinematic\n  orchestral "),
            "epic cinematic orchestral"
        );
    }

    #[test]
    fn normalize_block_preserves_line_structure() {
        let input = "[Verse]\r\nTiếng   trống\n\n\n[Chorus]\nVang đêm";
        assert_eq!(
            normalize_block(input),
            "[Verse]\nTiếng trống\n[Chorus]\nVang đêm"
        );
    }
}
