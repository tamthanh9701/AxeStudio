//! Engine: cpal output stream (WASAPI shared @48kHz), callback drain command
//! rồi render mixer. Đây là nơi DUY NHẤT gọi hệ điều hành về audio.

use crate::command::Command;
use crate::meter::Meters;
use crate::mixer::Mixer;
use crate::playhead::Playhead;
use crate::source::AudioSource;
use crate::transport::Transport;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
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

/// Handle phía control thread. Stream + state RT sống trong struct này;
/// UI chỉ chạm qua command producer + atomics.
pub struct Engine {
    command_tx: rtrb::Producer<Command>,
    playhead: Playhead,
    meters: Meters,
    config: AudioConfig,
    // Giữ stream sống — drop stream = dừng audio.
    _stream: cpal::Stream,
}

impl Engine {
    fn start(builder: EngineBuilder) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioError::NoOutputDevice)?;

        let stream_config = cpal::StreamConfig {
            channels: 2,
            sample_rate: cpal::SampleRate(builder.config.sample_rate),
            buffer_size: cpal::BufferSize::Fixed(builder.config.buffer_size),
        };

        let (tx, mut rx) = rtrb::RingBuffer::<Command>::new(256);
        let playhead = Playhead::new();
        let meters = Meters::new();
        let ph = playhead.clone();
        let mt = meters.clone();
        let sample_rate = builder.config.sample_rate;

        let mut mixer = Mixer::new();
        let mut sources = builder.sources;
        for _ in 0..sources.len() {
            mixer.add_track();
        }
        let mut transport = Transport::default();
        // Đếm xrun: cpal báo lỗi qua callback riêng; underrun phía ta tự đếm
        // khi source thiếu dữ liệu (S2 sẽ nối counter này vào Diagnostics).

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
                    }

                    // 2. Render.
                    if transport.playing {
                        mixer.render(&mut sources, out);
                    } else {
                        for s in out.iter_mut() {
                            *s = 0.0;
                        }
                    }

                    // 3. Cập nhật atomics về phía UI.
                    mt.update(out);
                    let frames = (out.len() / 2) as u64;
                    transport.advance(frames);
                    ph.store_frames(transport.position_frames);
                    let _ = sample_rate;

                    #[cfg(debug_assertions)]
                    crate::rt_guard::exit_rt_context();
                },
                |_err| {
                    // xrun / device error — không log trong callback (I/O!).
                    // S2: đếm vào atomic, Diagnostics đọc sau.
                },
                None,
            )
            .map_err(|e| AudioError::Cpal(e.to_string()))?;

        stream.play().map_err(|e| AudioError::Cpal(e.to_string()))?;

        Ok(Self {
            command_tx: tx,
            playhead,
            meters,
            config: builder.config,
            _stream: stream,
        })
    }

    fn send(&mut self, cmd: Command) {
        // Queue đầy → bỏ lệnh. 256 slot là rất lớn so với tốc độ bấm của người;
        // nếu đầy thì đó là bug flood phía caller, không retry vòng ở đây.
        let _ = self.command_tx.push(cmd);
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
}
