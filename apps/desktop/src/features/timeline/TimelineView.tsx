/**
 * Timeline v1.5 — waveform thật từ peaks (canvas per clip), click-to-seek,
 * zoom, ruler ticks. Sprint 4 thay renderer bằng @als/timeline (PixiJS) khi
 * cần 60fps với 24 track/200 clip; luồng dữ liệu (snapshot + peaks + playhead
 * poll) giữ nguyên.
 */
import { useEffect, useRef } from "react"
import { layout as layoutTokens } from "@als/ui"
import { ipc } from "../../ipc/client"
import { useStudio } from "../../state/store"
import { getPeaks } from "./peaksCache"
import type { Clip } from "@als/bindings"

function drawWaveform(canvas: HTMLCanvasElement, pairs: [number, number][]) {
  const ctx = canvas.getContext("2d")
  if (!ctx) return
  const w = canvas.width
  const h = canvas.height
  if (w <= 0 || h <= 0) return
  ctx.clearRect(0, 0, w, h)
  ctx.fillStyle = "#c9c4ff"
  const mid = h / 2
  const n = pairs.length
  if (n === 0) return
  for (let x = 0; x < w; x++) {
    const i = Math.min(n - 1, Math.floor((x / w) * n))
    const [lo, hi] = pairs[i] ?? [0, 0]
    const y1 = mid - hi * (mid - 1)
    const y2 = mid - lo * (mid - 1)
    ctx.fillRect(x, y1, 1, Math.max(1, y2 - y1))
  }
}

function ClipView(props: { clip: Clip; pxPerMs: number; selected: boolean }) {
  const { clip, pxPerMs, selected } = props
  const canvasRef = useRef<HTMLCanvasElement | null>(null)
  const take = useStudio((s) => s.takes[clip.id]?.find((t) => t.id === clip.active_take))
  const assetId = take?.asset_id ?? null
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
      title={clip.active_take ? "đã có take" : "chưa có take — Generate"}
    >
      <canvas ref={canvasRef} className="clip-wave" />
      {!clip.active_take && <span className="clip-empty">…</span>}
    </button>
  )
}

export function TimelineView() {
  const snapshot = useStudio((s) => s.snapshot)
  const playheadMs = useStudio((s) => s.playheadMs)
  const selectedClipId = useStudio((s) => s.selectedClipId)
  const pxPerMs = useStudio((s) => s.pxPerMs)
  const setPxPerMs = useStudio((s) => s.setPxPerMs)

  // Nạp take list cho mọi clip có active_take mà store chưa cache.
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
          <div className="track-header" style={{ width: layoutTokens.trackHeaderWidth }}>
            <span>{track.name}</span>
            <span className="dim">{track.kind}</span>
          </div>
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
