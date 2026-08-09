/**
 * Timeline v1.5 — waveform thật từ peaks (canvas per clip), click-to-seek,
 * zoom, ruler ticks, điều khiển track (mute/solo/gain). Sprint 4 thay renderer
 * bằng @als/timeline (PixiJS); luồng dữ liệu (snapshot + peaks + playhead
 * poll) giữ nguyên.
 */
import { useEffect, useRef, useState } from "react"
import { layout as layoutTokens } from "@als/ui"
import { ipc } from "../../ipc/client"
import { useStudio } from "../../state/store"
import { getPeaks } from "./peaksCache"
import { drawWaveform } from "./waveform"
import type { Clip, EditCommand, Track } from "@als/bindings"

function applyTrackEdit(cmd: EditCommand) {
  void ipc
    .applyEdit(cmd)
    .then((out) => useStudio.getState().setSnapshot(out.snapshot))
    .catch(() => {})
}

function TrackHeader({ track }: { track: Track }) {
  // Gain: kéo là preview local, chỉ commit (vào undo stack) khi thả chuột/blur —
  // tránh spam hàng chục edit cho một lần kéo slider.
  const [gain, setGain] = useState(track.gain_db)
  useEffect(() => setGain(track.gain_db), [track.gain_db])

  const commitGain = () => {
    if (gain !== track.gain_db) {
      applyTrackEdit({ op: "set_track_gain", track_id: track.id, gain_db: gain })
    }
  }

  return (
    <div className="track-header" style={{ width: layoutTokens.trackHeaderWidth }}>
      <div className="track-title">
        <span>{track.name}</span>
        <span className="dim">{track.kind}</span>
      </div>
      <div className="track-controls">
        <button
          className={track.mute ? "tctl tctl-on" : "tctl"}
          title="Mute"
          onClick={() =>
            applyTrackEdit({ op: "set_track_mute", track_id: track.id, mute: !track.mute })
          }
        >
          M
        </button>
        <button
          className={track.solo ? "tctl tctl-on" : "tctl"}
          title="Solo"
          onClick={() =>
            applyTrackEdit({ op: "set_track_solo", track_id: track.id, solo: !track.solo })
          }
        >
          S
        </button>
        <input
          type="range"
          min={-24}
          max={6}
          step={0.5}
          value={gain}
          title={`${gain.toFixed(1)} dB`}
          onChange={(e) => setGain(Number(e.target.value))}
          onPointerUp={commitGain}
          onBlur={commitGain}
        />
      </div>
    </div>
  )
}

function ClipView(props: { clip: Clip; pxPerMs: number; selected: boolean }) {
  const { clip, pxPerMs, selected } = props
  const canvasRef = useRef<HTMLCanvasElement | null>(null)
  const take = useStudio((s) => s.takes[clip.id]?.find((t) => t.id === clip.active_take))
  // Clip import không có take — lấy asset thẳng từ source.
  const assetId = take?.asset_id ?? (clip.source.type === "imported" ? clip.source.asset : null)
  const widthPx = Math.max(10, clip.duration_ms * pxPerMs)

  useEffect(() => {
    let alive = true
    if (!assetId) return
    void getPeaks(assetId).then((pv) => {
      if (!alive || !pv) return
      const canvas = canvasRef.current
      if (!canvas) return
      canvas.width = Math.max(1, Math.floor(canvas.clientWidth))
      canvas.height = Math.max(1, Math.floor(canvas.clientHeight))
      drawWaveform(canvas, pv.pairs)
    })
    return () => {
      alive = false
    }
  }, [assetId, widthPx])

  return (
    <button
      className={
        "clip" +
        (clip.source.type === "generated" ? " clip-generated" : " clip-imported") +
        (selected ? " clip-selected" : "")
      }
      style={{ left: clip.start_ms * pxPerMs, width: widthPx }}
      onClick={(e) => {
        e.stopPropagation()
        useStudio.getState().selectClip(clip.id)
      }}
      title={assetId ? "có audio" : "chưa có take — Generate"}
    >
      <canvas ref={canvasRef} className="clip-wave" />
      {!assetId && <span className="clip-empty">…</span>}
    </button>
  )
}

export function TimelineView() {
  const snapshot = useStudio((s) => s.snapshot)
  const playheadMs = useStudio((s) => s.playheadMs)
  const selectedClipId = useStudio((s) => s.selectedClipId)
  const pxPerMs = useStudio((s) => s.pxPerMs)
  const setPxPerMs = useStudio((s) => s.setPxPerMs)

  // Nạp take list cho mọi clip generate có active_take mà store chưa cache.
  useEffect(() => {
    if (!snapshot) return
    for (const track of snapshot.arrangement.tracks) {
      for (const clip of track.clips) {
        if (clip.active_take && useStudio.getState().takes[clip.id] === undefined) {
          void ipc
            .takeList(clip.id)
            .then((t) => useStudio.getState().setTakes(clip.id, t))
            .catch(() => {})
        }
      }
    }
  }, [snapshot])

  if (!snapshot) {
    return (
      <div className="timeline timeline-empty">
        <p>Mở hoặc tạo một project để bắt đầu.</p>
      </div>
    )
  }

  const totalMs = Math.max(
    60_000,
    ...snapshot.arrangement.tracks.flatMap((t) =>
      t.clips.map((c) => c.start_ms + c.duration_ms + 10_000),
    ),
  )

  const seekAt = (e: {
    clientX: number
    currentTarget: { getBoundingClientRect(): { left: number } }
  }) => {
    const rect = e.currentTarget.getBoundingClientRect()
    const ms = Math.max(0, (e.clientX - rect.left) / pxPerMs)
    void ipc.transportSeek(Math.round(ms)).catch(() => {})
  }

  return (
    <div className="timeline">
      <div className="timeline-toolbar">
        <button onClick={() => setPxPerMs(Math.max(0.005, pxPerMs * 0.8))}>−</button>
        <button onClick={() => setPxPerMs(Math.min(0.5, pxPerMs * 1.25))}>+</button>
        <span className="dim">{(pxPerMs * 1000).toFixed(0)} px/s</span>
      </div>
      <div className="ruler" style={{ marginLeft: layoutTokens.trackHeaderWidth }} onClick={seekAt}>
        {Array.from({ length: Math.ceil(totalMs / 5000) + 1 }, (_, i) => (
          <span key={i} className="tick" style={{ left: i * 5000 * pxPerMs }}>
            {i * 5}s
          </span>
        ))}
      </div>
      <div
        className="playhead"
        style={{ left: layoutTokens.trackHeaderWidth + playheadMs * pxPerMs }}
      />
      {snapshot.arrangement.tracks.length === 0 && (
        <p className="dim timeline-hint">
          Chưa có track nào — dùng panel Generate bên phải để tạo clip đầu tiên.
        </p>
      )}
      {snapshot.arrangement.tracks.map((track) => (
        <div className="track" key={track.id} style={{ height: layoutTokens.trackHeight }}>
          <TrackHeader track={track} />
          <div className="track-lane" onClick={seekAt}>
            {track.clips.map((clip) => (
              <ClipView
                key={clip.id}
                clip={clip}
                pxPerMs={pxPerMs}
                selected={clip.id === selectedClipId}
              />
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}
