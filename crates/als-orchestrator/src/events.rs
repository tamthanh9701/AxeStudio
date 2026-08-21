//! Event phát về UI qua broadcast. src-tauri subscribe và map sang Tauri event
//! theo đúng tên trong docs/contracts/ipc.md.

use als_core::{AssetId, JobId, JobState, TakeId};
use als_provider::Progress;

#[derive(Debug, Clone)]
pub enum OrchEvent {
    /// → `job:state`
    JobState {
        job_id: JobId,
        state: JobState,
        error: Option<String>,
    },
    /// → `job:progress` (đã throttle ≤4 lần/giây ở nơi phát)
    Progress(Progress),
    /// Một take sẵn sàng để gắn vào clip. `cached = true` nghĩa là cache tầng 2
    /// trúng — take_id trỏ tới take ĐÃ TỒN TẠI (có thể của clip khác); UI gắn
    /// bằng SetActiveTake, KHÔNG tạo take mới (UNIQUE render_hash).
    TakeReady {
        job_id: JobId,
        clip_id: String,
        take_id: TakeId,
        cached: bool,
    },
    /// → `peaks:ready` — peaks của asset đã sẵn sàng để UI vẽ waveform.
    PeaksReady { asset_id: AssetId },
}
