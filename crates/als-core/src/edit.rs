//! Áp dụng EditCommand lên Arrangement + UndoStack.
//!
//! Undo dạng SNAPSHOT: arrangement MVP nhỏ (vài trăm clip), clone rẻ hơn nhiều
//! so với việc viết inverse cho từng command — và không bao giờ sai inverse.
//! Khi document lớn hơn (nghìn clip, automation dày), viết ADR chuyển sang
//! inverse-per-command.

use crate::document::{Arrangement, Clip, EditCommand, EditResult, Track};
use crate::error::{ErrorCode, IpcError};
use crate::id::{ClipId, TrackId};

fn not_found(what: &str) -> IpcError {
    IpcError::new(ErrorCode::Internal, format!("{what} không tồn tại"))
}

/// Vị trí (track_idx, clip_idx) của một clip trong arrangement.
fn locate(arr: &Arrangement, clip_id: &ClipId) -> Option<(usize, usize)> {
    arr.tracks.iter().enumerate().find_map(|(ti, t)| {
        t.clips
            .iter()
            .position(|c| &c.id == clip_id)
            .map(|ci| (ti, ci))
    })
}

fn clip_mut(arr: &mut Arrangement, clip_id: &ClipId) -> Option<&mut Clip> {
    arr.tracks
        .iter_mut()
        .flat_map(|t| t.clips.iter_mut())
        .find(|c| &c.id == clip_id)
}

fn track_mut(arr: &mut Arrangement, track_id: &TrackId) -> Option<&mut Track> {
    arr.tracks.iter_mut().find(|t| &t.id == track_id)
}

/// Áp dụng một command. Pure theo nghĩa: chỉ đụng vào `arr`, không I/O.
pub fn apply(arr: &mut Arrangement, cmd: &EditCommand) -> Result<EditResult, IpcError> {
    match cmd {
        EditCommand::AddTrack { kind, name } => {
            arr.tracks.push(Track {
                id: TrackId::new(),
                kind: *kind,
                name: name.clone(),
                gain_db: 0.0,
                pan: 0.0,
                mute: false,
                solo: false,
                clips: vec![],
            });
            Ok(EditResult {
                label: format!("Thêm track {name}"),
            })
        }
        EditCommand::RemoveTrack { track_id } => {
            let before = arr.tracks.len();
            arr.tracks.retain(|t| &t.id != track_id);
            if arr.tracks.len() == before {
                return Err(not_found("track"));
            }
            Ok(EditResult {
                label: "Xoá track".into(),
            })
        }
        EditCommand::MoveClip {
            clip_id,
            to_track,
            start_ms,
        } => {
            let (ti, ci) = locate(arr, clip_id).ok_or_else(|| not_found("clip"))?;
            let mut clip = arr.tracks[ti].clips.remove(ci);
            clip.start_ms = *start_ms;
            let target = track_mut(arr, to_track).ok_or_else(|| not_found("track đích"))?;
            target.clips.push(clip);
            // Clip trên track luôn sort theo start_ms — renderer phụ thuộc vào điều này.
            target.clips.sort_by_key(|c| c.start_ms);
            Ok(EditResult {
                label: "Di chuyển clip".into(),
            })
        }
        EditCommand::TrimClip {
            clip_id,
            start_ms,
            duration_ms,
            offset_ms,
        } => {
            if *duration_ms == 0 {
                return Err(IpcError::new(
                    ErrorCode::InvalidRecipe,
                    "duration_ms phải > 0",
                ));
            }
            let clip = clip_mut(arr, clip_id).ok_or_else(|| not_found("clip"))?;
            clip.start_ms = *start_ms;
            clip.duration_ms = *duration_ms;
            clip.offset_ms = *offset_ms;
            Ok(EditResult {
                label: "Trim clip".into(),
            })
        }
        EditCommand::SplitClip { clip_id, at_ms } => {
            let (ti, ci) = locate(arr, clip_id).ok_or_else(|| not_found("clip"))?;
            let track = &mut arr.tracks[ti];
            let clip = &track.clips[ci];
            let clip_end = clip.start_ms + clip.duration_ms;
            if *at_ms <= clip.start_ms || *at_ms >= clip_end {
                return Err(IpcError::new(
                    ErrorCode::Internal,
                    "điểm split phải nằm TRONG clip",
                ));
            }
            let mut right = clip.clone();
            right.id = ClipId::new();
            right.start_ms = *at_ms;
            right.duration_ms = clip_end - *at_ms;
            right.offset_ms = clip.offset_ms + (*at_ms - clip.start_ms);

            let left = &mut track.clips[ci];
            left.duration_ms = *at_ms - left.start_ms;
            track.clips.push(right);
            track.clips.sort_by_key(|c| c.start_ms);
            Ok(EditResult {
                label: "Split clip".into(),
            })
        }
        EditCommand::SetTrackGain { track_id, gain_db } => {
            track_mut(arr, track_id)
                .ok_or_else(|| not_found("track"))?
                .gain_db = *gain_db;
            Ok(EditResult {
                label: "Đổi gain track".into(),
            })
        }
        EditCommand::SetTrackPan { track_id, pan } => {
            let t = track_mut(arr, track_id).ok_or_else(|| not_found("track"))?;
            t.pan = pan.clamp(-1.0, 1.0);
            Ok(EditResult {
                label: "Đổi pan track".into(),
            })
        }
        EditCommand::SetTrackMute { track_id, mute } => {
            track_mut(arr, track_id)
                .ok_or_else(|| not_found("track"))?
                .mute = *mute;
            Ok(EditResult {
                label: "Mute/unmute track".into(),
            })
        }
        EditCommand::SetTrackSolo { track_id, solo } => {
            track_mut(arr, track_id)
                .ok_or_else(|| not_found("track"))?
                .solo = *solo;
            Ok(EditResult {
                label: "Solo/unsolo track".into(),
            })
        }
        EditCommand::SetActiveTake { clip_id, take_id } => {
            clip_mut(arr, clip_id)
                .ok_or_else(|| not_found("clip"))?
                .active_take = Some(take_id.clone());
            Ok(EditResult {
                label: "Chọn take".into(),
            })
        }
        EditCommand::RemoveClip { clip_id } => {
            let (ti, ci) = locate(arr, clip_id).ok_or_else(|| not_found("clip"))?;
            arr.tracks[ti].clips.remove(ci);
            Ok(EditResult {
                label: "Xoá clip".into(),
            })
        }
    }
}

/// Undo/redo stack snapshot-based. Mỗi apply thành công đẩy (label, snapshot
/// TRƯỚC edit) vào undo và xoá redo — chuẩn hành vi editor.
#[derive(Default)]
pub struct UndoStack {
    undo: Vec<(String, Arrangement)>,
    redo: Vec<(String, Arrangement)>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(
        &mut self,
        arr: &mut Arrangement,
        cmd: &EditCommand,
    ) -> Result<EditResult, IpcError> {
        let before = arr.clone();
        let result = apply(arr, cmd)?;
        self.undo.push((result.label.clone(), before));
        self.redo.clear();
        Ok(result)
    }

    /// Trả label của thao tác vừa undo (để hiện toast), None nếu stack rỗng.
    pub fn undo(&mut self, arr: &mut Arrangement) -> Option<String> {
        let (label, prev) = self.undo.pop()?;
        self.redo.push((label.clone(), arr.clone()));
        *arr = prev;
        Some(label)
    }

    pub fn redo(&mut self, arr: &mut Arrangement) -> Option<String> {
        let (label, next) = self.redo.pop()?;
        self.undo.push((label.clone(), arr.clone()));
        *arr = next;
        Some(label)
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{ClipSource, TrackKind};

    fn one_clip_arrangement() -> (Arrangement, TrackId, ClipId) {
        let track_id = TrackId::new();
        let clip_id = ClipId::new();
        let arr = Arrangement {
            tracks: vec![Track {
                id: track_id.clone(),
                kind: TrackKind::Generated,
                name: "T1".into(),
                gain_db: 0.0,
                pan: 0.0,
                mute: false,
                solo: false,
                clips: vec![Clip {
                    id: clip_id.clone(),
                    start_ms: 0,
                    duration_ms: 10_000,
                    offset_ms: 0,
                    gain_db: 0.0,
                    fade_in_ms: 0,
                    fade_out_ms: 0,
                    source: ClipSource::Generated,
                    generation: None,
                    active_take: None,
                }],
            }],
        };
        (arr, track_id, clip_id)
    }

    #[test]
    fn split_is_sample_accurate_to_ms() {
        let (mut arr, _t, clip) = one_clip_arrangement();
        apply(
            &mut arr,
            &EditCommand::SplitClip {
                clip_id: clip.clone(),
                at_ms: 4_000,
            },
        )
        .unwrap();
        let clips = &arr.tracks[0].clips;
        assert_eq!(clips.len(), 2);
        assert_eq!(clips[0].duration_ms, 4_000);
        assert_eq!(clips[1].start_ms, 4_000);
        assert_eq!(clips[1].duration_ms, 6_000);
        assert_eq!(clips[1].offset_ms, 4_000, "phần phải trỏ đúng offset nguồn");
    }

    #[test]
    fn split_outside_clip_rejected() {
        let (mut arr, _t, clip) = one_clip_arrangement();
        assert!(apply(
            &mut arr,
            &EditCommand::SplitClip {
                clip_id: clip.clone(),
                at_ms: 10_000, // mép phải — không hợp lệ
            },
        )
        .is_err());
        assert!(apply(&mut arr, &EditCommand::SplitClip { clip_id: clip, at_ms: 0 }).is_err());
    }

    #[test]
    fn move_clip_keeps_track_sorted() {
        let (mut arr, track, clip) = one_clip_arrangement();
        // Thêm clip thứ hai ở đầu timeline, rồi move clip gốc về sau nó.
        let second = ClipId::new();
        arr.tracks[0].clips.push(Clip {
            id: second,
            start_ms: 20_000,
            duration_ms: 5_000,
            offset_ms: 0,
            gain_db: 0.0,
            fade_in_ms: 0,
            fade_out_ms: 0,
            source: ClipSource::Generated,
            generation: None,
            active_take: None,
        });
        apply(
            &mut arr,
            &EditCommand::MoveClip {
                clip_id: clip,
                to_track: track,
                start_ms: 30_000,
            },
        )
        .unwrap();
        let starts: Vec<u64> = arr.tracks[0].clips.iter().map(|c| c.start_ms).collect();
        assert_eq!(starts, vec![20_000, 30_000]);
    }

    #[test]
    fn undo_redo_roundtrip() {
        let (mut arr, _t, clip) = one_clip_arrangement();
        let mut stack = UndoStack::new();
        stack
            .apply(
                &mut arr,
                &EditCommand::TrimClip {
                    clip_id: clip.clone(),
                    start_ms: 1_000,
                    duration_ms: 5_000,
                    offset_ms: 500,
                },
            )
            .unwrap();
        assert_eq!(arr.tracks[0].clips[0].start_ms, 1_000);

        let label = stack.undo(&mut arr);
        assert_eq!(label.as_deref(), Some("Trim clip"));
        assert_eq!(arr.tracks[0].clips[0].start_ms, 0);

        stack.redo(&mut arr);
        assert_eq!(arr.tracks[0].clips[0].start_ms, 1_000);
        assert_eq!(stack.redo_len(), 0);
    }

    #[test]
    fn failed_edit_does_not_push_undo() {
        let (mut arr, _t, _c) = one_clip_arrangement();
        let mut stack = UndoStack::new();
        let ghost = ClipId::new();
        let r = stack.apply(&mut arr, &EditCommand::RemoveClip { clip_id: ghost });
        assert!(r.is_err());
        assert_eq!(stack.undo_len(), 0, "edit fail không được để lại snapshot");
    }
}
