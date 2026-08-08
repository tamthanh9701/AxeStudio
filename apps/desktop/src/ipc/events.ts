/**
 * Listen wrapper cho event Rust → UI. Tên event khớp docs/contracts/ipc.md.
 * listen() trả Promise<UnlistenFn> — caller giữ và gọi khi unmount.
 */
import { listen } from "@tauri-apps/api/event"
import type {
  JobProgressEvent,
  JobStateEvent,
  PeaksReadyEvent,
  ProjectDirtyEvent,
  TakeReadyEvent,
} from "@als/bindings"

export function onJobState(cb: (e: JobStateEvent) => void) {
  return listen<JobStateEvent>("job:state", (ev) => cb(ev.payload))
}

export function onJobProgress(cb: (e: JobProgressEvent) => void) {
  return listen<JobProgressEvent>("job:progress", (ev) => cb(ev.payload))
}

export function onTakeReady(cb: (e: TakeReadyEvent) => void) {
  return listen<TakeReadyEvent>("take:ready", (ev) => cb(ev.payload))
}

export function onPeaksReady(cb: (e: PeaksReadyEvent) => void) {
  return listen<PeaksReadyEvent>("peaks:ready", (ev) => cb(ev.payload))
}

export function onProjectDirty(cb: (dirty: boolean) => void) {
  return listen<ProjectDirtyEvent>("project:dirty", (ev) => cb(ev.payload.dirty))
}
