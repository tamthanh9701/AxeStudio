//! Engine: cpal output stream (WASAPI shared @48kHz), callback drain command
//! rồi render mixer. Đây là nơi DUY NHẤT gọi hệ điều hành về audio.
//!
//! ## Vì sao stream sống trong thread riêng
//! `cpal::Stream` trên Windows (WASAPI) giữ HANDLE thô nên KHÔNG `Send`/`Sync`.
//! Nếu `Engine` giữ thẳng field đó, `AppState` của Tauri mất `Send + Sync` và
//! kéo theo E0277 hàng loạt: `.manage()`, mọi `#[tauri::command]` nhận
//! `State<'_, AppState>`, `spawn_blocking` trả `Engine`, `app.state()` trong
//! task forward event. Nên: stream được TẠO, `play()` và DROP trong đúng một
//! audio thread; `Engine` chỉ là handle gồm producer command + atomics →
//! `Engine: Send`, đi qua biên async/thread tự do.
//! (Đây chính là pattern đã ghi chú sẵn ở src-tauri/state.rs.)
//!
//! Bonus: WASAPI thích được gọi từ cùng một thread trong suốt đời stream, nên
//! cách này cũng đúng hơn về mặt COM.
//!
//! Đồng bộ source: sources ở v1 là buffer CONSOLIDATED timeline-absolute
//! (xem src-tauri/player.rs), nên Seek/Stop/loop-wrap phải seek cả sources —
//! nếu không transport nhảy mà audio vẫn chạy chỗ cũ.

use crate::command::Command;
use crate::meter::Meters;
use crate::mixer::Mixer;
use crate::playhead::Playhead;
use crate::source::AudioSource;
use crate::transport::Transport;
use crate::xrun::XrunCounter;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioConfig {
    pub sample_rate: u32,
    /// 256 / 512 / 1024 — mặc định 512 theo perf budget.
    pub buffer_size: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            buffer_size: 512,
        }
    }
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("không có thiết bị output")]
    NoOutputDevice,
    #[error("cpal: {0}")]
    Cpal(String),
}

type SourceSlot = Option<Box<dyn AudioSource>>;

/// Đăng ký source lúc build — xem command.rs vì sao không AddTrack lúc runtime.
pub struct EngineBuilder {
    pub config: AudioConfig,
    pub sources: Vec<SourceSlot>,
}

impl EngineBuilder {
    pub fn new(config: AudioConfig) -> Self {
        Self {
            config,
            sources: Vec::new(),
        }
    }

    pub fn with_source(mut self, source: Box<dyn AudioSource>) -> Self {
        self.sources.push(Some(source));
        self
    }

    pub fn start(self) -> Result<Engine, AudioError> {
        Engine::start(self)
    }
}

/// Handle phía control thread. `Send` — KHÔNG giữ `cpal::Stream` (xem doc đầu
/// file). UI chỉ chạm engine qua command producer + atomics.
pub struct Engine {
    command_tx: rtrb::Producer<Command>,
    playhead: Playhead,
    meters: Meters,
    xruns: XrunCounter,
    config: AudioConfig,
    /// Báo audio thread dừng để stream được drop ở đúng thread đã tạo nó.
    shutdown_tx: Option<mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Tạo + chạy stream. PHẢI gọi bên trong audio thread: mọi lời gọi cpal nằm ở
/// đây, và stream cũng sẽ được drop tại chính thread này.
fn build_stream(
    config: AudioConfig,
    mut sources: Vec<SourceSlot>,
    mut rx: rtrb::Consumer<Command>,
    ph: Playhead,
    mt: Meters,
    xr: XrunCounter,
) -> Result<cpal::Stream, AudioError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or(AudioError::NoOutputDevice)?;

    let stream_config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(config.sample_rate),
        buffer_size: cpal::BufferSize::Fixed(config.buffer_size),
    };

    let mut mixer = Mixer::new();
    for _ in 0..sources.len() {
        // add_track trả Option (#[must_use]): quá 32 track thì trả None và track
        // đó im lặng không phát — giới hạn đã ghi trong mixer.rs.
        let _ = mixer.add_track();
    }
    let mut transport = Transport::default();
    // Xrun/underrun đếm vào `xr`: cpal error callback + underrun sau render
    // (định nghĩa underrun xem trong callback). Spike S-08 đọc counter này.

    // Clone riêng cho error callback — data callback bên dưới đã move `xr`.
    let xr_err = xr.clone();
    let stream = device
        .build_output_stream(
            &stream_config,
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                #[cfg(debug_assertions)]
                crate::rt_guard::enter_rt_context();

                // 1. Drain mọi command đang chờ — hành động của user thấy
                //    ngay trong block kế tiếp.
                while let Ok(cmd) = rx.pop() {
                    transport.apply(&cmd);
                    mixer.apply(&cmd);
                    // Đồng bộ sources theo transport. Buffer consolidated là
                    // timeline-absolute → seek thẳng theo frame tuyệt đối.
                    match cmd {
                        Command::Seek(f) => {
                            for s in sources.iter_mut().flatten() {
                                s.seek_frames(f);
                            }
                        }
                        Command::Stop => {
                            for s in sources.iter_mut().flatten() {
                                s.seek_frames(0);
                            }
                        }
                        _ => {}
                    }
                }

                // 2. Render.
                if transport.playing {
                    mixer.render(&mut sources, out);
                } else {
                    for s in out.iter_mut() {
                        *s = 0.0;
                    }
                }
                // Underrun phía ta: đang phát nhưng MỌI source đều cạn →
                // silence nghe được. Đếm là xrun, không panic (AGENTS.md §3).
                if transport.playing
                    && !sources.is_empty()
                    && sources
                        .iter()
                        .all(|s| s.as_ref().is_none_or(|x| x.is_finished()))
                {
                    xr.bump();
                }

                // 3. Cập nhật atomics về phía UI + xử lý loop wrap.
                mt.update(out);
                let frames = (out.len() / 2) as u64;
                let before = transport.position_frames;
                transport.advance(frames);
                let after = transport.position_frames;
                if after < before {
                    // Vừa wrap loop → sources phải quay về đầu loop.
                    for s in sources.iter_mut().flatten() {
                        s.seek_frames(after);
                    }
                }
                ph.store_frames(after);

                #[cfg(debug_assertions)]
                crate::rt_guard::exit_rt_context();
            },
            move |_err| {
                // xrun / device error — không log trong callback (I/O!).
                // Chỉ đếm vào atomic; Diagnostics/spike S-08 đọc sau.
                xr_err.bump();
            },
            None,
        )
        .map_err(|e| AudioError::Cpal(e.to_string()))?;

    stream.play().map_err(|e| AudioError::Cpal(e.to_string()))?;
    Ok(stream)
}

impl Engine {
    fn start(builder: EngineBuilder) -> Result<Self, AudioError> {
        let config = builder.config;
        let sources = builder.sources;
        let (command_tx, command_rx) = rtrb::RingBuffer::<Command>::new(256);
        let playhead = Playhead::new();
        let meters = Meters::new();
        let ph = playhead.clone();
        let mt = meters.clone();
        let xruns = XrunCounter::new();
        let xr = xruns.clone();

        // Kết quả mở device phải quay về control thread: người dùng cần thấy
        // lỗi "không có thiết bị output" ngay, không phải im lặng mất tiếng.
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), AudioError>>();
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

        let thread = std::thread::Builder::new()
            .name("als-audio".to_owned())
            .spawn(move || {
                match build_stream(config, sources, command_rx, ph, mt, xr) {
                    Ok(stream) => {
                        if ready_tx.send(Ok(())).is_err() {
                            // Control thread bỏ đi giữa đường → dừng luôn.
                            return;
                        }
                        // Chặn tới khi Engine bị drop. recv() lỗi = sender đã
                        // drop → cũng là tín hiệu dừng.
                        let _ = shutdown_rx.recv();
                        drop(stream);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })
            .map_err(|e| AudioError::Cpal(format!("không spawn được audio thread: {e}")))?;

        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = thread.join();
                return Err(e);
            }
            Err(_) => {
                let _ = thread.join();
                return Err(AudioError::Cpal(
                    "audio thread chết khi khởi tạo".to_owned(),
                ));
            }
        }

        Ok(Self {
            command_tx,
            playhead,
            meters,
            xruns,
            config,
            shutdown_tx: Some(shutdown_tx),
            thread: Some(thread),
        })
    }

    fn send(&mut self, cmd: Command) {
        // Queue đầy → bỏ lệnh. 256 slot là rất lớn so với tốc độ bấm của người;
        // nếu đầy thì đó là bug flood phía caller, không retry vòng ở đây.
        let _ = self.command_tx.push(cmd);
    }

    /// Gửi command tự do (gain/pan/mute/solo...). Dùng khi rebuild engine xong
    /// cần đẩy lại toàn bộ tham số track từ arrangement.
    pub fn send_command(&mut self, cmd: Command) {
        self.send(cmd);
    }

    pub fn play(&mut self) {
        self.send(Command::Play);
    }
    pub fn pause(&mut self) {
        self.send(Command::Pause);
    }
    pub fn stop(&mut self) {
        self.send(Command::Stop);
    }
    pub fn seek_frames(&mut self, frames: u64) {
        self.send(Command::Seek(frames));
    }
    pub fn seek_ms(&mut self, ms: u64) {
        self.seek_frames(ms * u64::from(self.config.sample_rate) / 1000);
    }
    pub fn set_loop(&mut self, start_ms: u64, end_ms: u64, enabled: bool) {
        let sr = u64::from(self.config.sample_rate);
        self.send(Command::SetLoop {
            start_frames: start_ms * sr / 1000,
            end_frames: end_ms * sr / 1000,
            enabled,
        });
    }

    pub fn playhead(&self) -> Playhead {
        self.playhead.clone()
    }
    pub fn meters(&self) -> Meters {
        self.meters.clone()
    }
    /// Bộ đếm xrun/underrun — spike S-08 và Diagnostics đọc định kỳ.
    pub fn xruns(&self) -> XrunCounter {
        self.xruns.clone()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // Dừng audio thread rồi CHỜ nó xong: stream phải được drop trước khi
        // process tháo COM/WASAPI, nếu không có thể treo lúc thoát app.
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_ms_converts_to_frames() {
        let cfg = AudioConfig::default();
        // 1000ms @48kHz = 48000 frames.
        assert_eq!(1000u64 * u64::from(cfg.sample_rate) / 1000, 48_000);
    }

    /// Engine PHẢI là Send: nó nằm trong AppState của Tauri (Send + Sync) và đi
    /// qua spawn_blocking. Test này compile-fail ngay nếu ai đó nhét lại
    /// cpal::Stream (hoặc thứ !Send khác) vào struct.
    #[test]
    fn engine_handle_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Engine>();
    }
}
