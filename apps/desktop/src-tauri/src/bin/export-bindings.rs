//! Sinh `packages/bindings/src/generated.ts` từ Rust types.
//!
//! Chạy từ thư mục gốc repo:
//!   cargo run -p als-desktop --bin export-bindings
//! (hoặc `pnpm bindings:generate`). CI job bindings-drift fail nếu quên chạy.

fn main() {
    let builder = als_desktop_lib::specta_builder();
    let path = std::path::Path::new("packages/bindings/src/generated.ts");
    builder
        .export(specta_typescript::Typescript::default(), path)
        .expect("export bindings thất bại");
    println!("đã sinh {}", path.display());
}
