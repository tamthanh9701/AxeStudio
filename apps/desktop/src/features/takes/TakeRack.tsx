/**
 * Take rack — danh sách take của clip đang chọn, mới nhất trên cùng.
 * Mỗi take có mini-waveform từ peaks, star, promote (đổi take active qua
 * undo stack), delete. A/B nghe bằng cách promote rồi Play.
 */
import { useEffect, useRef } from "react"
import { explainError, ipc } from "../../ipc/client"
import { useStudio } from "../../state/store"
import { getPeaks } from "../timeline/peaksCache"
import { drawWaveform } from "../timeline/waveform"

function TakeWave({ assetId }: { assetId: string }) {
  const ref = useRef<HTMLCanvasElement | null>(null)
  useEffect(() => {
    let alive = true
    void getPeaks(assetId).then((pv) => {
      if (!alive || !pv) return
      const canvas = ref.current
      if (!canvas) return
      canvas.width = Math.max(1, Math.floor(canvas.clientWidth))
      canvas.height = Math.max(1, Math.floor(canvas.clientHeight))
      drawWaveform(canvas, pv.pairs)
    })
    return () => {
      alive = false
    }
  }, [assetId])
  return <canvas ref={ref} className="take-wave" />
}

export function TakeRack() {
  const selectedClipId = useStudio((s) => s.selectedClipId)
  const takes = useStudio((s) => (s.selectedClipId ? (s.takes[s.selectedClipId] ?? null) : null))

  useEffect(() => {
    if (!selectedClipId) return
    ipc
      .takeList(selectedClipId)
      .then((t) => useStudio.getState().setTakes(selectedClipId, t))
      .catch(() => {})
  }, [selectedClipId])

  if (!selectedClipId) {
    return (
      <div className="panel">
        <h3>Take rack</h3>
        <p className="dim">Chọn một clip trên timeline để xem các take.</p>
      </div>
    )
  }

  const promote = async (takeId: string) => {
    try {
      const out = await ipc.takePromote(selectedClipId, takeId)
      useStudio.getState().setSnapshot(out.snapshot)
    } catch (e) {
      alert(explainError(e))
    }
  }

  return (
    <div className="panel">
      <h3>Take rack</h3>
      {!takes || takes.length === 0 ? (
        <p className="dim">Clip chưa có take nào — bấm Generate.</p>
      ) : (
        <ul className="take-list">
          {takes.map((t, i) => (
            <li key={t.id} className={t.starred ? "take take-starred" : "take"}>
              <div className="take-head">
                <span>
                  Take {takes.length - i}
                  {t.lufs !== null && <span className="dim"> · {t.lufs.toFixed(1)} LUFS</span>}
                </span>
                <span className="take-actions">
                  <button
                    title={t.starred ? "Bỏ star" : "Star"}
                    onClick={() =>
                      void ipc.takeStar(t.id, !t.starred).then(() => {
                        const list = takes.map((x) =>
                          x.id === t.id ? { ...x, starred: !t.starred } : x,
                        )
                        useStudio.getState().setTakes(selectedClipId, list)
                      })
                    }
                  >
                    {t.starred ? "★" : "☆"}
                  </button>
                  <button onClick={() => void promote(t.id)}>Dùng</button>
                  <button
                    className="danger"
                    onClick={() =>
                      void ipc.takeDelete(t.id).then(() => {
                        useStudio.getState().setTakes(
                          selectedClipId,
                          takes.filter((x) => x.id !== t.id),
                        )
                      })
                    }
                  >
                    ×
                  </button>
                </span>
              </div>
              <TakeWave assetId={t.asset_id} />
              <div className="dim take-meta">
                {new Date(t.created_at_unix * 1000).toLocaleString("vi-VN")}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
