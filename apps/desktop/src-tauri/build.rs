//! Build script Tauri.
//!
//! Ngoài `tauri_build::build()`, file này ĐẢM BẢO tồn tại `icons/icon.ico`.
//! Lý do: trên Windows, tauri-build LUÔN sinh một Windows Resource file và
//! mặc định trỏ vào `icons/icon.ico` — kể cả khi `bundle.icon` để rỗng. Thiếu
//! file đó thì build script exit 1 với:
//!   "`icons/icon.ico` not found; required for generating a Windows Resource
//!    file during tauri-build"
//! Đây là fail lúc BUILD, không phải lúc bundle, nên `cargo test` cũng chết.
//!
//! Icon thật (bộ đầy đủ + branding) là việc của Sprint 7. Ở đây chỉ sinh một
//! placeholder 1×1 hợp lệ KHI CHƯA CÓ file, để máy vừa clone về là build được.
//! Đã có icon thật → guard `exists()` giữ nguyên, không bao giờ ghi đè.

use std::path::Path;

/// ICO tối thiểu hợp lệ: 1 ảnh 1×1, 32bpp BGRA + AND mask.
/// Layout: ICONDIR(6) + ICONDIRENTRY(16) + BITMAPINFOHEADER(40) + pixel(4) + mask(4).
const PLACEHOLDER_ICO: &[u8] = &[
    // ICONDIR: reserved=0, type=1 (icon), count=1
    0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
    // ICONDIRENTRY: width=1, height=1, colorCount=0, reserved=0, planes=1, bpp=32
    0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x20, 0x00, // bytesInRes = 48
    0x30, 0x00, 0x00, 0x00, // imageOffset = 22
    0x16, 0x00, 0x00, 0x00, // BITMAPINFOHEADER: biSize = 40
    0x28, 0x00, 0x00, 0x00, // biWidth = 1
    0x01, 0x00, 0x00, 0x00,
    // biHeight = 2 — ICO nhân đôi chiều cao: XOR bitmap + AND mask
    0x02, 0x00, 0x00, 0x00, // biPlanes = 1, biBitCount = 32
    0x01, 0x00, 0x20, 0x00, // biCompression = BI_RGB (0), biSizeImage = 4
    0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
    // biXPelsPerMeter, biYPelsPerMeter, biClrUsed, biClrImportant = 0
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // Pixel BGRA — accent tím #7c6cf2 của app
    0xF2, 0x6C, 0x7C, 0xFF, // AND mask: 1 hàng 1bpp, pad tới 4 byte
    0x00, 0x00, 0x00, 0x00,
];

fn ensure_placeholder_icon() {
    let icon = Path::new("icons/icon.ico");
    if icon.exists() {
        return;
    }
    if let Some(dir) = icon.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            println!("cargo:warning=không tạo được {}: {e}", dir.display());
            return;
        }
    }
    match std::fs::write(icon, PLACEHOLDER_ICO) {
        Ok(()) => println!(
            "cargo:warning=đã sinh icons/icon.ico placeholder 1×1 — thay bằng icon thật ở Sprint 7"
        ),
        Err(e) => println!("cargo:warning=không ghi được icons/icon.ico: {e}"),
    }
}

fn main() {
    ensure_placeholder_icon();
    tauri_build::build()
}
