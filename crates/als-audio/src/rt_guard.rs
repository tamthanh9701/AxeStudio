//! rt_guard — rào chắn realtime ở mức thư viện.
//!
//! Thực thi thật gồm 3 lớp:
//! 1. `clippy.toml` cấm `std::sync::Mutex` và `std::thread::sleep` toàn workspace.
//! 2. `tests/no_alloc.rs` dùng counting allocator CHỨNG MINH mixer không cấp phát.
//! 3. Module này cung cấp marker tài liệu + hook debug.
//!
//! Attribute-like marker giúp người review (và agent) nhận ra hàm chạy trong
//! callback chỉ bằng một cú grep `rt_safe!`.

/// Đánh dấu một closure/chạy-một-lần là RT path. Hiện tại là identity —
/// giá trị của nó nằm ở khả năng grep và ở chỗ test no_alloc bao trọn mixer.
#[inline(always)]
pub fn rt_safe<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    f()
}

/// Gọi ở ĐẦU audio callback ở debug build. Đặt thread-local flag để các lớp
/// instrumentation tương lai (vd allocator guard toàn app) biết đang ở RT thread.
#[cfg(debug_assertions)]
#[inline]
pub fn enter_rt_context() {
    RT_CONTEXT.with(|c| c.set(true));
}

#[cfg(debug_assertions)]
#[inline]
pub fn exit_rt_context() {
    RT_CONTEXT.with(|c| c.set(false));
}

#[cfg(debug_assertions)]
thread_local! {
    static RT_CONTEXT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// True nếu đang trong RT context (chỉ có ở debug build).
#[cfg(debug_assertions)]
#[inline]
pub fn in_rt_context() -> bool {
    RT_CONTEXT.with(|c| c.get())
}
