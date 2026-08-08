//! Golden buffer test: mixer phải khớp BYTE với một reference implementation
//! naive viết độc lập trong test. Mọi thay đổi mixer mà làm lệch kết quả số
//! học đều bị bắt ở đây — kể cả "refactor nhỏ".

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

#[test]
fn mixer_matches_naive_reference_bit_exact() {
    const FRAMES: usize = 512;

    let data_a = sine(440.0, FRAMES, 48_000.0);
    let data_b = sine(550.0, FRAMES, 48_000.0);

    let mut mixer = Mixer::new();
    let ta = mixer.add_track().unwrap();
    let tb = mixer.add_track().unwrap();
    // Pan mặc định center → gain_l = gain_r = cos(π/4) = sin(π/4).
    let g = std::f32::consts::FRAC_PI_4.cos();

    let mut sources: Vec<Option<Box<dyn AudioSource>>> = vec![
        Some(Box::new(BufferSource::from_interleaved(data_a.clone()))),
        Some(Box::new(BufferSource::from_interleaved(data_b.clone()))),
    ];
    let _ = (ta, tb);

    let mut out = vec![0.0f32; FRAMES * 2];
    mixer.render(&mut sources, &mut out);

    let reference = naive_mix(&[(g, g, 0.0), (g, g, 0.0)], &[data_a, data_b], FRAMES);
    assert_eq!(
        out.len(),
        reference.len(),
        "độ dài output khác nhau"
    );
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

    // Mute track a → chỉ nghe b.
    mixer.tracks[a].mute = true;
    let mut out = vec![0.0f32; FRAMES * 2];
    mixer.render(&mut sources, &mut out);
    let g = std::f32::consts::FRAC_PI_4.cos();
    assert!((out[0] - 0.5 * g).abs() < f32::EPSILON * 4.0);

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
    assert!((out2[0] - 0.5 * g).abs() < f32::EPSILON * 4.0);
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
    assert!(out.iter().all(|&s| s <= 1.0 && s >= -1.0));
}
