//! Metering: peak + RMS per master, qua ATOMIC — không qua event IPC.
//! f32 được nhét vào AtomicU32 dạng bits (f32::to_bits/from_bits).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MeterReading {
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
}

#[derive(Clone)]
pub struct Meters {
    inner: Arc<Inner>,
}

struct Inner {
    peak_l: AtomicU32,
    peak_r: AtomicU32,
    rms_l: AtomicU32,
    rms_r: AtomicU32,
}

impl Meters {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                peak_l: AtomicU32::new(0),
                peak_r: AtomicU32::new(0),
                rms_l: AtomicU32::new(0),
                rms_r: AtomicU32::new(0),
            }),
        }
    }

    /// Audio thread: cập nhật sau mỗi block. RMS tính lũy tiến đơn giản
    /// trên block — đủ cho meter UI (cửa sổ chuẩn BS.1770 nằm ở als-media,
    /// không nằm trên đường realtime).
    #[inline]
    pub fn update(&self, block: &[f32]) {
        let mut pl = 0.0f32;
        let mut pr = 0.0f32;
        let mut sl = 0.0f32;
        let mut sr = 0.0f32;
        let frames = block.len() / 2;
        for f in 0..frames {
            let l = block.get(f * 2).copied().unwrap_or(0.0);
            let r = block.get(f * 2 + 1).copied().unwrap_or(0.0);
            pl = pl.max(l.abs());
            pr = pr.max(r.abs());
            sl += l * l;
            sr += r * r;
        }
        let n = (frames.max(1)) as f32;
        self.inner.peak_l.store(pl.to_bits(), Ordering::Release);
        self.inner.peak_r.store(pr.to_bits(), Ordering::Release);
        self.inner
            .rms_l
            .store((sl / n).sqrt().to_bits(), Ordering::Release);
        self.inner
            .rms_r
            .store((sr / n).sqrt().to_bits(), Ordering::Release);
    }

    /// UI thread: đọc trong rAF.
    pub fn read(&self) -> MeterReading {
        MeterReading {
            peak_l: f32::from_bits(self.inner.peak_l.load(Ordering::Acquire)),
            peak_r: f32::from_bits(self.inner.peak_r.load(Ordering::Acquire)),
            rms_l: f32::from_bits(self.inner.rms_l.load(Ordering::Acquire)),
            rms_r: f32::from_bits(self.inner.rms_r.load(Ordering::Acquire)),
        }
    }
}

impl Default for Meters {
    fn default() -> Self {
        Self::new()
    }
}
