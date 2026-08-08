/**
 * Zustand store — trạng thái UI tập trung. Document (arrangement) luôn đến từ
 * snapshot Rust trả về sau mỗi edit; UI KHÔNG tự sửa arrangement phía client
 * (đó là cách tránh drift giữa hai phía).
 */
import { create } from "zustand"
import type {
  EngineStatus,
  JobState,
  ProgressStage,
  ProjectSnapshot,
  TakeInfo,
} from "@als/bindings"

export interface JobView {
  percent: number
  stage: ProgressStage
  state: JobState
  error?: string | undefined
}

interface StudioState {
  snapshot: ProjectSnapshot | null
  dirty: boolean
  engine: EngineStatus | null
  jobs: Record<string, JobView>
  playheadMs: number
  playing: boolean
  selectedClipId: string | null
  takes: Record<string, TakeInfo[]>
  /** Zoom timeline — pixel trên mỗi ms. 0.05 = 50px/giây. */
  pxPerMs: number

  setSnapshot: (s: ProjectSnapshot | null) => void
  setDirty: (d: boolean) => void
  setEngine: (e: EngineStatus) => void
  patchJob: (id: string, patch: Partial<JobView>) => void
  setPlayhead: (ms: number, playing: boolean) => void
  selectClip: (id: string | null) => void
  setTakes: (clipId: string, takes: TakeInfo[]) => void
  setPxPerMs: (v: number) => void
}

export const useStudio = create<StudioState>()((set) => ({
  snapshot: null,
  dirty: false,
  engine: null,
  jobs: {},
  playheadMs: 0,
  playing: false,
  selectedClipId: null,
  takes: {},
  pxPerMs: 0.05,

  setSnapshot: (snapshot) => set({ snapshot }),
  setDirty: (dirty) => set({ dirty }),
  setEngine: (engine) => set({ engine }),
  patchJob: (id, patch) =>
    set((s) => {
      const cur: JobView = s.jobs[id] ?? { percent: 0, stage: "queued", state: "queued" }
      return { jobs: { ...s.jobs, [id]: { ...cur, ...patch } } }
    }),
  setPlayhead: (playheadMs, playing) => set({ playheadMs, playing }),
  selectClip: (selectedClipId) => set({ selectedClipId }),
  setTakes: (clipId, takes) => set((s) => ({ takes: { ...s.takes, [clipId]: takes } })),
  setPxPerMs: (pxPerMs) => set({ pxPerMs }),
}))
