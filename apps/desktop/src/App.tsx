import { useEffect, useState } from "react"
import { explainError, ipc, SAMPLE_RATE } from "./ipc/client"
import { onJobProgress, onJobState, onProjectDirty, onTakeReady } from "./ipc/events"
import { useStudio } from "./state/store"
import { GeneratePanel } from "./features/generate/GeneratePanel"
import { TimelineView } from "./features/timeline/TimelineView"
import { TakeRack } from "./features/takes/TakeRack"

export default function App() {
  const snapshot = useStudio((s) => s.snapshot)
  const dirty = useStudio((s) => s.dirty)
  const engine = useStudio((s) => s.engine)
  const playing = useStudio((s) => s.playing)
  const playheadMs = useStudio((s) => s.playheadMs)
  const [path, setPath] = useState("D:\\AxeStudio\\demo.aiproj")
  const [name, setName] = useState("Demo")
  const [exportPath, setExportPath] = useState("D:\\AxeStudio\\export\\master.wav")
  const [importPaths, setImportPaths] = useState("")
  const [loopOn, setLoopOn] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)

  const doUndo = async () => {
    try {
      const r = await ipc.undo()
      if (r.snapshot) useStudio.getState().setSnapshot(r.snapshot)
    } catch (e) {
      setNotice(explainError(e))
    }
  }

  const doRedo = async () => {
    try {
      const r = await ipc.redo()
      if (r.snapshot) useStudio.getState().setSnapshot(r.snapshot)
    } catch (e) {
      setNotice(explainError(e))
    }
  }

  // Event subscriptions — mount một lần.
  useEffect(() => {
    const subs = [
      onJobState((e) => {
        useStudio.getState().patchJob(e.job_id, { state: e.state, error: e.error ?? undefined })
        // Notification cho hàng đợi render (#14): job dài không còn "ẩn" —
        // thất bại phải nói rõ, không để người dùng chờ vô nghĩa.
        if (!e.job_id.startsWith("warm:") && e.state === "failed") {
          setNotice(`Render thất bại: ${e.error ?? "lỗi không rõ"}`)
        }
        if (e.job_id.startsWith("warm:") && e.state === "failed") {
          setNotice("Không nạp sẵn được model — lượt generate đầu sẽ chậm hơn")
        }
      }),
      onJobProgress((e) =>
        useStudio.getState().patchJob(e.job_id, { percent: e.percent, stage: e.stage }),
      ),
      onTakeReady((e) => {
        // Gắn take vào clip + refresh take rack.
        void ipc
          .takePromote(e.clip_id, e.take_id)
          .then((out) => useStudio.getState().setSnapshot(out.snapshot))
          .catch(() => {})
        void ipc
          .takeList(e.clip_id)
          .then((t) => useStudio.getState().setTakes(e.clip_id, t))
          .catch(() => {})
      }),
      onProjectDirty((d) => useStudio.getState().setDirty(d)),
    ]
    const enginePoll = window.setInterval(() => {
      void ipc
        .engineStatus()
        .then((s) => useStudio.getState().setEngine(s))
        .catch(() => {})
    }, 2000)
    return () => {
      subs.forEach((p) => void p.then((unsub) => unsub()))
      window.clearInterval(enginePoll)
    }
  }, [])

  // Playhead: poll transport_position trong rAF — đúng contract, KHÔNG event 60fps.
  useEffect(() => {
    let raf = 0
    const tick = () => {
      void ipc
        .transportPosition()
        .then((p) => useStudio.getState().setPlayhead((p.frames / SAMPLE_RATE) * 1000, p.playing))
        .catch(() => {})
      raf = requestAnimationFrame(tick)
    }
    raf = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(raf)
  }, [])

  // Phím tắt undo/redo. doUndo/doRedo chỉ đọc ipc + getState nên deps rỗng an toàn.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement | null)?.tagName
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return
      if (!e.ctrlKey && !e.metaKey) return
      const key = e.key.toLowerCase()
      if (key === "z") {
        e.preventDefault()
        void (e.shiftKey ? doRedo() : doUndo())
      } else if (key === "y") {
        e.preventDefault()
        void doRedo()
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [])

  const open = async () => {
    try {
      const snap = await ipc.projectOpen(path)
      useStudio.getState().setSnapshot(snap)
      setNotice(null)
    } catch (e) {
      setNotice(explainError(e))
    }
  }

  const create = async () => {
    try {
      const snap = await ipc.projectCreate(path, name)
      useStudio.getState().setSnapshot(snap)
      setNotice(null)
    } catch (e) {
      setNotice(explainError(e))
    }
  }

  const switchBackend = async (providerId: string) => {
    try {
      await ipc.engineSwitchBackend(providerId)
      const s = await ipc.engineStatus()
      useStudio.getState().setEngine(s)
    } catch (e) {
      setNotice(explainError(e))
    }
  }

  const toggleLoop = () => {
    const snap = useStudio.getState().snapshot
    if (!snap) return
    const end = Math.max(
      0,
      ...snap.arrangement.tracks.flatMap((t) => t.clips.map((c) => c.start_ms + c.duration_ms)),
    )
    const next = !loopOn
    setLoopOn(next)
    void ipc.transportLoop(0, Math.max(end, 1000), next).catch(() => {})
  }

  const doImport = async () => {
    const paths = importPaths
      .split(";")
      .map((p) => p.trim())
      .filter((p) => p.length > 0)
    if (paths.length === 0) return
    setNotice(null)
    try {
      const ids = await ipc.assetImport(paths)
      let snap = useStudio.getState().snapshot
      if (!snap) return
      let trackId = snap.arrangement.tracks[0]?.id
      if (!trackId) {
        const out = await ipc.applyEdit({ op: "add_track", kind: "audio", name: "Imports" })
        snap = out.snapshot
        useStudio.getState().setSnapshot(snap)
        trackId = snap.arrangement.tracks[0]?.id
      }
      if (!trackId) throw new Error("không tạo được track")
      let cursor = 0
      for (const id of ids) {
        const info = await ipc.assetGet(id)
        const dur = info.duration_ms ?? 30_000
        const out = await ipc.applyEdit({
          op: "add_clip",
          track_id: trackId,
          clip_id: crypto.randomUUID(),
          start_ms: cursor,
          duration_ms: dur,
          source: { type: "imported", asset: id },
        })
        useStudio.getState().setSnapshot(out.snapshot)
        cursor += dur
      }
      setNotice(`Đã import ${ids.length} file`)
      setImportPaths("")
    } catch (e) {
      setNotice(explainError(e))
    }
  }

  const doExport = async () => {
    try {
      const out = await ipc.exportRender({
        format: "wav24",
        range: { type: "project" },
        out_path: exportPath,
        include_metadata: true,
      })
      setNotice(`Đã xuất: ${out}`)
    } catch (e) {
      setNotice(explainError(e))
    }
  }

  return (
    <div className="app">
      <header className="topbar">
        <span className="logo">AxeStudio</span>
        <input
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder="đường dẫn project"
        />
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder="tên project" />
        <button onClick={() => void open()}>Mở</button>
        <button onClick={() => void create()}>Tạo mới</button>
        <span className="spacer" />
        <button onClick={() => void doUndo()} title="Undo (Ctrl+Z)">
          ↶
        </button>
        <button onClick={() => void doRedo()} title="Redo (Ctrl+Y)">
          ↷
        </button>
        <select
          value={engine?.backend ?? "mock"}
          onChange={(e) => void switchBackend(e.target.value)}
          title="Backend engine"
        >
          <option value="mock">mock (dev)</option>
          <option value="cpp">acestep.cpp</option>
          <option value="py">acestep-api</option>
        </select>
        <button
          className={loopOn ? "engine engine-ok" : "engine"}
          onClick={toggleLoop}
          title="Loop toàn project"
        >
          Loop
        </button>
        <button onClick={() => void ipc.transportPlay().catch(() => {})}>▶</button>
        <button onClick={() => void ipc.transportPause().catch(() => {})}>⏸</button>
        <span className="dim">
          {(playheadMs / 1000).toFixed(1)}s {playing ? "· đang phát" : ""}
        </span>
        <span className={engine?.ready ? "engine engine-ok" : "engine engine-off"}>
          {engine ? `${engine.backend}${engine.ready ? "" : " (chưa sẵn sàng)"}` : "…"}
        </span>
      </header>

      {notice && <div className="notice">{notice}</div>}

      <main className="main">
        <section className="center">
          {snapshot && (
            <div className="project-title">
              {snapshot.name} {dirty && <span className="dim">• chưa lưu</span>}
            </div>
          )}
          <TimelineView />
          {snapshot && (
            <>
              <div className="import-bar">
                <input
                  value={importPaths}
                  onChange={(e) => setImportPaths(e.target.value)}
                  placeholder="import audio — đường dẫn tuyệt đối, cách nhau bởi dấu ;"
                />
                <button onClick={() => void doImport()}>Import</button>
              </div>
              <div className="export-bar">
                <input
                  value={exportPath}
                  onChange={(e) => setExportPath(e.target.value)}
                  placeholder="đường dẫn file WAV xuất ra"
                />
                <button onClick={() => void doExport()}>Export WAV</button>
              </div>
            </>
          )}
        </section>
        <aside className="inspector">
          <GeneratePanel />
          <TakeRack />
        </aside>
      </main>
    </div>
  )
}
