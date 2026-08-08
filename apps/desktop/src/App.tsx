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
  const [notice, setNotice] = useState<string | null>(null)

  // Event subscriptions — mount một lần.
  useEffect(() => {
    const subs = [
      onJobState((e) =>
        useStudio.getState().patchJob(e.job_id, { state: e.state, error: e.error ?? undefined }),
      ),
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
        <input value={path} onChange={(e) => setPath(e.target.value)} placeholder="đường dẫn project" />
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder="tên project" />
        <button onClick={() => void open()}>Mở</button>
        <button onClick={() => void create()}>Tạo mới</button>
        <span className="spacer" />
        <select
          value={engine?.backend ?? "mock"}
          onChange={(e) => void switchBackend(e.target.value)}
          title="Backend engine"
        >
          <option value="mock">mock (dev)</option>
          <option value="cpp">acestep.cpp</option>
          <option value="py">acestep-api</option>
        </select>
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
            <div className="export-bar">
              <input
                value={exportPath}
                onChange={(e) => setExportPath(e.target.value)}
                placeholder="đường dẫn file WAV xuất ra"
              />
              <button onClick={() => void doExport()}>Export WAV</button>
            </div>
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
