//! Document model — phần dữ liệu người dùng chỉnh sửa.
//!
//! Layout project (ADR-004 + plan §5): `manifest.json` GIỮ document (track/clip),
//! SQLite chỉ là INDEX (asset, take, job, cache). Document là thứ người dùng
//! quý nhất — nó phải đọc được bằng mắt và diff được bằng git nếu cần.

use crate::id::{AssetId, ClipId, ProjectId, TakeId, TrackId};
use crate::recipe::GenerationRecipe;
use serde::{Deserialize, Serialize};

/// Version schema của project file. Tăng MAJOR app version khi đổi schema (ADR-004).
pub const PROJECT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    /// Audio import từ ngoài.
    Audio,
    /// Track chứa clip do engine sinh ra.
    Generated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Track {
    pub id: TrackId,
    pub kind: TrackKind,
    pub name: String,
    pub gain_db: f32,
    /// -1.0 (trái) ..= 1.0 (phải)
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub clips: Vec<Clip>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClipSource {
    Imported {
        asset: AssetId,
    },
    /// Clip sinh bởi engine — audio thật nằm ở take active, không ở đây.
    Generated,
}

/// Lineage của clip generate: recipe đầy đủ để tái lập (plan §5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct GenerationInfo {
    pub recipe: GenerationRecipe,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Clip {
    pub id: ClipId,
    pub start_ms: u64,
    pub duration_ms: u64,
    /// Offset vào audio nguồn (trim đầu).
    pub offset_ms: u64,
    pub gain_db: f32,
    pub fade_in_ms: u64,
    pub fade_out_ms: u64,
    pub source: ClipSource,
    pub generation: Option<GenerationInfo>,
    /// Take đang phát. None = chưa generate hoặc take cuối bị xoá.
    pub active_take: Option<TakeId>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, specta::Type)]
pub struct Arrangement {
    pub tracks: Vec<Track>,
}

/// Snapshot gửi qua IPC khi open/create project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ProjectSnapshot {
    pub project_id: ProjectId,
    pub name: String,
    pub schema_version: u32,
    pub arrangement: Arrangement,
    pub dirty: bool,
}

/// Thông tin một take trả về UI (take rack). Tách khỏi TakeRow nội bộ của
/// als-project: IPC chỉ thấy kiểu này.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct TakeInfo {
    pub id: TakeId,
    pub clip_id: String,
    pub plan_hash: String,
    pub render_hash: String,
    pub asset_id: AssetId,
    /// Integrated loudness (LUFS), None khi postprocess đo lỗi.
    pub lufs: Option<f64>,
    pub true_peak_db: Option<f64>,
    pub starred: bool,
    pub created_at_unix: i64,
}

/// Lệnh chỉnh sửa từ UI. Mỗi lệnh phải undo được — kể cả thao tác async (plan §5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EditCommand {
    AddTrack {
        kind: TrackKind,
        name: String,
    },
    RemoveTrack {
        track_id: TrackId,
    },
    /// clip_id do CLIENT sinh (uuid) — generate_submit cần tham chiếu clip
    /// ngay, không chờ vòng IPC quay lại.
    AddClip {
        track_id: TrackId,
        clip_id: ClipId,
        start_ms: u64,
        duration_ms: u64,
        source: ClipSource,
    },
    MoveClip {
        clip_id: ClipId,
        to_track: TrackId,
        start_ms: u64,
    },
    TrimClip {
        clip_id: ClipId,
        start_ms: u64,
        duration_ms: u64,
        offset_ms: u64,
    },
    SplitClip {
        clip_id: ClipId,
        at_ms: u64,
    },
    SetTrackGain {
        track_id: TrackId,
        gain_db: f32,
    },
    SetTrackPan {
        track_id: TrackId,
        pan: f32,
    },
    SetTrackMute {
        track_id: TrackId,
        mute: bool,
    },
    SetTrackSolo {
        track_id: TrackId,
        solo: bool,
    },
    SetActiveTake {
        clip_id: ClipId,
        take_id: TakeId,
    },
    RemoveClip {
        clip_id: ClipId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct EditResult {
    /// Nhãn hiển thị trong menu Undo, vd "Move clip".
    pub label: String,
}
