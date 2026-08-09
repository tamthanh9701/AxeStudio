/**
 * Wrapper DUY NHẤT gọi IPC. Component CẤM gọi invoke trực tiếp (AGENTS.md §7).
 * Tên tham số camelCase — Tauri tự chuyển sang snake_case phía Rust.
 *
 * Mọi kiểu dữ liệu đến từ @als/bindings — CẤM định nghĩa lại ở đây.
 */
import { invoke } from "@tauri-apps/api/core"
import type {
  AssetId,
  AssetInfo,
  CancelOutcome,
  EditCommand,
  EditOutcome,
  EngineStatus,
  ExportSpec,
  GenerationRecipe,
  PeakView,
  ProjectSnapshot,
  TakeInfo,
  TransportPosition,
  UndoOutcome,
} from "@als/bindings"

// Tiện cho component import một chỗ — vẫn là re-export, không phải định nghĩa mới.
export type { PeakView, UndoOutcome } from "@als/bindings"

/** Sample rate chuẩn toàn hệ thống (plan §5 — chốt 48kHz). */
export const SAMPLE_RATE = 48_000

export const ipc = {
  // project
  projectCreate: (path: string, name: string) =>
    invoke<ProjectSnapshot>("project_create", { path, name }),
  projectOpen: (path: string) => invoke<ProjectSnapshot>("project_open", { path }),
  projectSaveAs: (path: string) => invoke<ProjectSnapshot>("project_save_as", { path }),
  applyEdit: (cmd: EditCommand) => invoke<EditOutcome>("project_apply_edit", { cmd }),
  undo: () => invoke<UndoOutcome>("project_undo"),
  redo: () => invoke<UndoOutcome>("project_redo"),

  // asset
  assetImport: (paths: string[]) => invoke<AssetId[]>("asset_import", { paths }),
  assetGet: (assetId: string) => invoke<AssetInfo>("asset_get", { assetId }),
  assetPeaks: (assetId: string, zoomLevel: number) =>
    invoke<PeakView>("asset_peaks", { assetId, zoomLevel }),

  // generation
  generateSubmit: (clipId: string, recipe: GenerationRecipe, priority?: number) =>
    invoke<string>("generate_submit", { clipId, recipe, priority }),
  jobCancel: (jobId: string) => invoke<CancelOutcome>("job_cancel", { jobId }),
  takeList: (clipId: string) => invoke<TakeInfo[]>("take_list", { clipId }),
  takePromote: (clipId: string, takeId: string) =>
    invoke<EditOutcome>("take_promote", { clipId, takeId }),
  takeStar: (takeId: string, starred: boolean) => invoke<void>("take_star", { takeId, starred }),
  takeDelete: (takeId: string) => invoke<void>("take_delete", { takeId }),

  // transport
  transportPlay: () => invoke<void>("transport_play"),
  transportPause: () => invoke<void>("transport_pause"),
  transportSeek: (positionMs: number) => invoke<void>("transport_seek", { positionMs }),
  transportLoop: (startMs: number, endMs: number, enabled: boolean) =>
    invoke<void>("transport_loop", { startMs, endMs, enabled }),
  transportPosition: () => invoke<TransportPosition>("transport_position"),

  // engine
  engineStatus: () => invoke<EngineStatus>("engine_status"),
  engineSwitchBackend: (providerId: string) =>
    invoke<void>("engine_switch_backend", { providerId }),

  // export — v1 đồng bộ, trả đường dẫn file ra (contract ipc.md §Export)
  exportRender: (spec: ExportSpec) => invoke<string>("export_render", { spec }),
}

/**
 * Map ErrorCode → thông báo tiếng Việt. KHÔNG hiển thị IpcError.message thô
 * cho người dùng cuối (contract ipc.md §Quy ước lỗi).
 */
export function explainError(e: unknown): string {
  const code = (e as { code?: string } | null)?.code
  switch (code) {
    case "PROJECT_NOT_FOUND":
      return "Chưa mở project — hãy mở hoặc tạo project trước"
    case "SCHEMA_TOO_NEW":
      return "Project này được tạo bởi phiên bản AxeStudio mới hơn — hãy cập nhật app"
    case "PROJECT_CORRUPT":
      return "Project bị lỗi — thử mở bản backup hoặc báo lỗi kèm Diagnostics"
    case "ASSET_NOT_FOUND":
      return "Asset chưa sẵn sàng hoặc không tồn tại"
    case "ASSET_IN_USE":
      return "Asset đang được tham chiếu bởi take/clip khác"
    case "PROVIDER_UNAVAILABLE":
      return "Engine chưa sẵn sàng — kiểm tra backend trong trang Diagnostics"
    case "CAPABILITY_NOT_SUPPORTED":
      return "Backend hiện tại không hỗ trợ tính năng này"
    case "JOB_NOT_FOUND":
      return "Job không tồn tại hoặc đã kết thúc"
    case "JOB_TOO_LATE":
      return "Job đã gửi xuống engine — không huỷ được nữa, hãy chờ kết quả"
    case "MODEL_MISSING":
      return "Model chưa được tải về máy"
    case "VRAM_EXHAUSTED":
      return "Hết VRAM — đóng bớt job hoặc chọn model nhỏ hơn"
    case "INVALID_RECIPE":
      return "Tham số sinh nhạc chưa hợp lệ"
    case "EXPORT_FAILED":
      return "Export thất bại"
    case "IO":
      return "Lỗi đọc/ghi file"
    default:
      return "Có lỗi xảy ra — mở Diagnostics để xem chi tiết"
  }
}
