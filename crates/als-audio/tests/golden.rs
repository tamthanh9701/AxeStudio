//! Golden buffer test: mixer phải khớp BYTE với một reference implementation
//! naive viết độc lập trong test. Mọi thay đổi mixer mà làm lệch kết quả số
//! học đều bị bắt ở đây — kể cả "refactor nhỏ".
//!
//! ĐỌC TRƯỚC KHI SỬA — vì sao reference phải gọi cos cho L và sin cho R:
//! mixer tính gL = cos((pan+1)·π/4), gR = sin((pan+1)·π/4). Ở pan = 0 thì góc
//! bằng π/4 và cos(π/4) = sin(π/4) VỀ MẶT TOÁN HỌC. Nhưng `FRAC_PI_4` trong
//! f32 lớn hơn π/4 thật khoảng 2.19e-8, nên cos giảm và sin tăng cùng một
//! lượng ≈ 0.26 ULP quanh √2/2. Cả hai vẫn làm tròn về cùng một f32 —
//! nhưng phía sin chỉ còn ≈ 0.038 ULP dư trước biên làm tròn. Nếu reference
//! dùng cos cho kênh phải rồi so byte, ta không còn kiểm tra mixer nữa mà
//! đang đánh cược vào độ chính xác của `sinf`/`cosf` trong libm của toolchain.
//! Vì vậy: reference gọi ĐÚNG hàm mà mixer gọi cho từng kênh.

use als_audio::mixer::Mixer;
use als_audio::source::{AudioSource, BufferSource};

/// Reference naive: cùng công thức, viết theo cách không thể tối ưu hoá nhầm.
fn naive_mix(
    tracks: &[(f32, f32, f32)], // (gain_l, gain_r, _unused)
    sources: &[Vec<f32>],
    frames: usize,
) -> Vec<f32> {
    let mut out = vec![0.0f32; frames * 2];
    for (t, src) in tracks.iter().zip(sources) {
        for f in 0..frames {
            let l = src.get(f * 2).copied().unwrap_or(0.0);
            let r = src.get(f * 2 + 1).copied().unwrap_or(0.0);
            out[f * 2] += l * t.0;
            out[f * 2 + 1] += r * t.1;
        }
    }
    for s in out.iter_mut() {
        *s = s.clamp(-1.0, 1.0);
    }
    out
}

fn sine(freq: f32, frames: usize, sr: f32) -> Vec<f32> {
    let mut v = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let s = (2.0 * std::f32::consts::PI * freq * (i as f32 / sr)).sin() * 0.25;
        v.push(s);
        v.push(s);
    }
    v
}

/// Gain của pan center, lấy đúng cách mixer lấy — xem doc đầu file.
fn center_gains() -> (f32, f32) {
    let angle = std::f32::consts::FRAC_PI_4;
    (angle.cos(), angle.sin())
}

/// Bất biến mà test bit-exact bên dưới dựa vào. Tách riêng để nếu một ngày
/// libm đổi và cos(π/4) != sin(π/4) trong f32, ta thấy MỘT dòng fail nói rõ
/// nguyên nhân, thay vì 1024 dòng "sample N lệch bit" không ai đọc nổi.
#[test]
fn center_pan_gains_are_bit_equal_in_f32() {
    let (gl, gr) = center_gains();
    assert_eq!(
        gl.to_bits(),
        gr.to_bits(),
        "cos(π/4) và sin(π/4) không còn cùng một f32 trên toolchain này \
         ({gl:e} vs {gr:e}). Mixer VẪN ĐÚNG — chỗ cần sửa là kỳ vọng L/R của \
         golden test, đừng đi sửa mixer."
    );
}

#[test]
fn mixer_matches_naive_reference_bit_exact() {
    const FRAMES: usize = 512;

    let data_a = sine(440.0, FRAMES, 48_000.0);
    let data_b = sine(550.0, FRAMES, 48_000.0);

    let mut mixer = Mixer::new();
    let ta = mixer.add_track().unwrap();
    let tb = mixer.add_track().unwrap();
    // Pan mặc định center.
    let (gl, gr) = center_gains();

    let mut sources: Vec<Option<Box<dyn AudioSource>>> = vec![
        Some(Box::new(BufferSource::from_interleaved(data_a.clone()))),
        Some(Box::new(BufferSource::from_interleaved(data_b.clone()))),
    ];
    let _ = (ta, tb);

    let mut out = vec![0.0f32; FRAMES * 2];
    mixer.render(&mut sources, &mut out);

    let reference = naive_mix(&[(gl, gr, 0.0), (gl, gr, 0.0)], &[data_a, data_b], FRAMES);
    assert_eq!(out.len(), reference.len(), "độ dài output khác nhau");
    // So BYTE, không so epsilon: mixer không được phép thay đổi kết quả số học
    // khi refactor. Nếu một ngày ta cố ý đổi thuật toán (vd limiter), test này
    // phải được viết lại trong PR đổi thuật toán đó.
    for (i, (a, b)) in out.iter().zip(reference.iter()).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "sample {i} lệch bit");
    }
}

#[test]
fn mute_and_solo_behave() {
    const FRAMES: usize = 64;
    let mut mixer = Mixer::new();
    let a = mixer.add_track().unwrap();
    let b = mixer.add_track().unwrap();

    let loud = vec![0.5f32; FRAMES * 2];
    let mut sources: Vec<Option<Box<dyn AudioSource>>> = vec![
        Some(Box::new(BufferSource::from_interleaved(loud.clone()))),
        Some(Box::new(BufferSource::from_interleaved(loud.clone()))),
    ];

    // Mute track a → chỉ nghe b. Kiểm tra CẢ HAI kênh: kênh phải dùng gr,
    // nếu ai đó đổi pan law mà chỉ sửa kênh trái thì phải fail ở đây.
    mixer.tracks[a].mute = true;
    let mut out = vec![0.0f32; FRAMES * 2];
    mixer.render(&mut sources, &mut out);
    let (gl, gr) = center_gains();
    assert!((out[0] - 0.5 * gl).abs() < f32::EPSILON * 4.0);
    assert!((out[1] - 0.5 * gr).abs() < f32::EPSILON * 4.0);

    // Solo a → b im, a hát (bỏ mute a trước).
    mixer.tracks[a].mute = false;
    mixer.tracks[a].solo = true;
    let mut sources2: Vec<Option<Box<dyn AudioSource>>> = vec![
        Some(Box::new(BufferSource::from_interleaved(loud.clone()))),
        Some(Box::new(BufferSource::from_interleaved(loud.clone()))),
    ];
    let _ = b;
    let mut out2 = vec![0.0f32; FRAMES * 2];
    mixer.render(&mut sources2, &mut out2);
    assert!((out2[0] - 0.5 * gl).abs() < f32::EPSILON * 4.0);
    assert!((out2[1] - 0.5 * gr).abs() < f32::EPSILON * 4.0);
}

#[test]
fn master_clamps_overdrive() {
    const FRAMES: usize = 8;
    let mut mixer = Mixer::new();
    mixer.add_track().unwrap();
    mixer.add_track().unwrap();
    let hot = vec![1.5f32; FRAMES * 2]; // > 1.0
    let mut sources: Vec<Option<Box<dyn AudioSource>>> = vec![
        Some(Box::new(BufferSource::from_interleaved(hot.clone()))),
        Some(Box::new(BufferSource::from_interleaved(hot))),
    ];
    let mut out = vec![0.0f32; FRAMES * 2];
    mixer.render(&mut sources, &mut out);
    assert!(out.iter().all(|&s| (-1.0..=1.0).contains(&s)));
}
