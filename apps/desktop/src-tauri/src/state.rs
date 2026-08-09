//! AppState — trạng thái sống cùng app.
//!
//! clippy.toml cấm std::sync::Mutex toàn workspace → mọi khóa ở đây là
//! tokio::sync::Mutex (async command). AppHandle đi qua OnceLock: set một lần
//! ở setup, đọc tự do sau đó.
//!
//! Ràng buộc cứng: Tauri yêu cầu state `Send + Sync + 'static` (`.manage()` và
//! mọi `State<'_, AppState>`). Vì thế MỌI field ở đây phải `Send`. Đừng nhét
//! handle của OS (cpal::Stream, HANDLE, COM pointer...) vào — als_audio::Engine
//! đã cố tình che stream sau một audio thread riêng để giữ được `Send`, và có
//! test `engine_handle_is_send` canh điều đó.

use als_audio::Engine;
use als_core::UndoStack;
use als_orchestrator::OrchestratorHandle;
use als_project::{Project, ProjectLayout};
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;
use tauri::AppHandle;
use tokio::sync::Mutex;

/// Alias để player/bounce nói về layout mà không import lẻ.
pub type Layout = ProjectLayout;

pub struct AppState {
    app: OnceLock<AppHandle>,
    pub project: Mutex<Option<Project>>,
    pub orchestrator: Mutex<Option<OrchestratorHandle>>,
    pub undo: Mutex<UndoStack>,
    /// Engine tạo lazy ở lần play đầu tiên và REBUILD khi sources đổi
    /// (player::refresh). Engine là handle Send: cpal::Stream sống trong audio
    /// thread của als-audio, không nằm ở đây.
    pub engine: Mutex<Option<Engine>>,
    /// Command-level playing flag (transport_position trả về cho UI poll).
    pub playing: AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            app: OnceLock::new(),
            project: Mutex::new(None),
            orchestrator: Mutex::new(None),
            undo: Mutex::new(UndoStack::new()),
            engine: Mutex::new(None),
            playing: AtomicBool::new(false),
        }
    }

    pub fn set_app(&self, app: AppHandle) {
        let _ = self.app.set(app);
    }

    pub fn handle(&self) -> &AppHandle {
        self.app.get().expect("AppHandle được set ở setup trước mọi command")
    }
}
