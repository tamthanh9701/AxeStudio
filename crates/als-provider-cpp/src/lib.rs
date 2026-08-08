//! als-provider-cpp — client cho `ace-server` của acestep.cpp (GGML/GGUF).
//!
//! Endpoints (xem https://github.com/ace-step/acestep.vst3):
//! - `GET  /props`           — health + thông tin server
//! - `POST /lm`              — pha LM: prompt/lyrics → audio_codes
//! - `POST /synth?wav=1`     — pha DiT+VAE: audio_codes → WAV bytes (48k stereo)
//! - `POST /understand`      — phân tích audio → caption/bpm/key
//!
//! ace-server KHÔNG có endpoint cancel → cancel() trả TooLate trung thực.
//! Field name chính xác của /lm và /synth cần spike S-01 xác nhận — mọi điểm
//! chưa chắc đều đánh dấu `TODO(S-01)`.

#![forbid(unsafe_code)]

mod client;
mod provider;

pub use client::AceServerClient;
pub use provider::CppProvider;
