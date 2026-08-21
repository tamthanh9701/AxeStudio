//! als-desktop — shell Tauri 2: đăng ký IPC command, forward event từ
//! orchestrator, giữ AppState. Logic nghiệp vụ nằm ở các crate `als-*`;
//! file này chỉ là lớp dán.

mod assets_io;
mod commands;
mod events;
mod player;
mod state;

pub use state::AppState;

/// Builder dùng chung cho `run()` và `bin/export-bindings` — một nguồn sự thật
/// cho bề mặt IPC (docs/contracts/ipc.md).
pub fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            commands::project_create,
            commands::project_open,
            commands::project_save_as,
            commands::project_apply_edit,
            commands::project_undo,
            commands::project_redo,
            commands::asset_import,
            commands::asset_get,
            commands::asset_peaks,
            commands::generate_submit,
            commands::job_cancel,
            commands::take_list,
            commands::take_promote,
            commands::take_star,
            commands::take_delete,
            commands::transport_play,
            commands::transport_pause,
            commands::transport_seek,
            commands::transport_loop,
            commands::transport_position,
            commands::engine_status,
            commands::engine_switch_backend,
            commands::export_render,
        ])
        .events(tauri_specta::collect_events![
            events::JobStateEvent,
            events::JobProgressEvent,
            events::TakeReadyEvent,
            events::PeaksReadyEvent,
            events::ProjectDirtyEvent,
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let builder = specta_builder();
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(builder.invoke_handler())
        .setup(|app| {
            use tauri::Manager;
            app.state::<AppState>().set_app(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("lỗi khởi động AxeStudio");
}
