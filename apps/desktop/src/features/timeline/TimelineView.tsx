/**
 * Timeline v1 — render HTML/CSS đơn giản. Sprint 4 thay bằng @als/timeline
 * (PixiJS) khi cần 60fps với 24 track/200 clip; cấu trúc props giữ nguyên.
 */
import { layout as layoutTokens } from "@als/ui"
import { useStudio } from "../../state/store"

const PX_PER_MS = 0.05 // 50px/giây — zoom sẽ đến ở S4

export function TimelineView() {
  const snapshot = useStudio((s) => s.snapshot)
  const playheadMs = useStudio((s) => s.playheadMs)
  const selectedClipId = useStudio((s) => s.selectedClipId)
  const selectClip = useStudio((s) => s.selectClip)

  if (!snapshot) {
    return (
      <div className="timeline timeline-empty">
        <p>Mở hoặc tạo một project để bắt đầu.</p>
      </div>
    )
  }

  return (
    <div className="timeline">
      <div className="ruler" style={{ marginLeft: layoutTokens.trackHeaderWidth }} />
      <div
        className="playhead"
        style={{ left: layoutTokens.trackHeaderWidth + playheadMs * PX_PER_MS }}
      />
      {snapshot.arrangement.tracks.length === 0 && (
        <p className="dim timeline-hint">
          Chưa có track nào — dùng panel Generate bên phải để tạo clip đầu tiên.
        </p>
      )}
      {snapshot.arrangement.tracks.map((track) => (
        <div className="track" key={track.id} style={{ height: layoutTokens.trackHeight }}>
          <div className="track-header" style={{ width: layoutTokens.trackHeaderWidth }}>
            <span>{track.name}</span>
            <span className="dim">{track.kind}</span>
          </div>
          <div className="track-lane">
            {track.clips.map((clip) => (
              <button
                key={clip.id}
                className={
                  "clip" +
                  (clip.source.type === "generated" ? " clip-generated" : " clip-imported") +
                  (clip.id === selectedClipId ? " clip-selected" : "")
                }
                style={{
                  left: clip.start_ms * PX_PER_MS,
                  width: Math.max(10, clip.duration_ms * PX_PER_MS),
                }}
                onClick={() => selectClip(clip.id)}
                title={clip.active_take ? "có take" : "chưa có take"}
              >
                {clip.active_take ? "♪" : "…"}
              </button>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}
