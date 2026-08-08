//! Chứng minh bằng counting allocator: mixer.render KHÔNG cấp phát heap.
//!
//! Đây là enforcement tầng 2 của luật realtime (rt_guard.rs). Một PR thêm
//! `Vec::push` hay `format!` vào đường render sẽ fail test này NGAY, kể cả
//! khi người review không để ý.

use als_audio::mixer::Mixer;
use als_audio::source::{AudioSource, BufferSource};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

struct CountingAlloc;

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::SeqCst);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static A: CountingAlloc = CountingAlloc;

#[test]
fn render_allocates_nothing() {
    const FRAMES: usize = 512;

    // Setup — ĐƯỢC phép cấp phát. Chỉ đo trong vùng render.
    let mut mixer = Mixer::new();
    mixer.add_track().unwrap();
    mixer.add_track().unwrap();
    let data = vec![0.1f32; FRAMES * 2];
    let mut sources: Vec<Option<Box<dyn AudioSource>>> = vec![
        Some(Box::new(BufferSource::from_interleaved(data.clone()))),
        Some(Box::new(BufferSource::from_interleaved(data))),
    ];
    let mut out = vec![0.0f32; FRAMES * 2];

    let before = ALLOC_COUNT.load(Ordering::SeqCst);
    // Render 1000 block liên tiếp — bắt cả allocation xảy ra "thỉnh thoảng".
    for _ in 0..1000 {
        mixer.render(&mut sources, &mut out);
    }
    let after = ALLOC_COUNT.load(Ordering::SeqCst);

    assert_eq!(
        before,
        after,
        "mixer.render cấp phát {} lần trong 1000 block — vi phạm luật realtime",
        after - before
    );
}
