/** 与 Rust 端 models.rs 对应的前端类型 */

export type TaskStatus =
  | "pending"
  | "connecting"
  | "downloading"
  | "paused"
  | "merging"
  | "completed"
  | "cancelled"
  | "failed";

export type ProxyMode = "system" | "none" | "custom";
export type FileExistsAction = "rename" | "overwrite" | "skip";
export type AfterFinishAction = "none" | "quit" | "shutdown" | "sleep";
export type ThemeMode = "system" | "dark" | "light";

export interface SegmentProgress {
  index: number;
  start: number;
  end: number;
  downloaded: number;
  done: boolean;
}

export interface DownloadTask {
  id: string;
  url: string;
  save_path: string;
  filename: string;
  total_size: number;
  downloaded: number;
  status: TaskStatus;
  segments: SegmentProgress[];
  connections: number;
  speed_limit: number;
  error: string | null;
  created_at: number;
  finished_at: number | null;
  user_agent: string;
  headers: [string, string][];
  resumable: boolean;
}

export interface ProgressEvent {
  id: string;
  downloaded: number;
  total_size: number;
  speed: number;
  status: TaskStatus;
  eta_secs: number | null;
  segments: SegmentProgress[];
}

export interface NewTaskRequest {
  url: string;
  dir: string;
  filename?: string | null;
  connections?: number | null;
  speed_limit?: number | null;
  user_agent?: string | null;
  headers?: [string, string][] | null;
  start_paused?: boolean | null;
}

export interface Settings {
  download_dir: string;
  connections: number;
  speed_limit: number;
  max_concurrent: number;
  enable_segment: boolean;
  segment_threshold: number;
  proxy_mode: ProxyMode;
  proxy_addr: string;
  user_agent: string;
  file_exists: FileExistsAction;
  confirm_remove_task: boolean;
  remove_task_files: boolean;
  after_finish: AfterFinishAction;
  close_to_tray: boolean;
  autostart: boolean;
  theme: ThemeMode;
}

/** URL 预解析结果 */
export interface ProbeInfo {
  filename: string;
  total_size: number;
  accept_ranges: boolean;
  ok: boolean;
  error: string;
}

export interface AppInfo {
  version: string;
  exe_path: string;
}

/** 默认设置（后端未返回时的兜底） */
export const DEFAULT_SETTINGS: Settings = {
  download_dir: "",
  connections: 8,
  speed_limit: 0,
  max_concurrent: 3,
  enable_segment: true,
  segment_threshold: 1024 * 1024,
  proxy_mode: "system",
  proxy_addr: "",
  user_agent: "",
  file_exists: "rename",
  confirm_remove_task: true,
  remove_task_files: false,
  after_finish: "none",
  close_to_tray: true,
  autostart: false,
  theme: "dark",
};

export const STATUS_LABEL: Record<TaskStatus, string> = {
  pending: "等待中",
  connecting: "连接中",
  downloading: "下载中",
  paused: "已暂停",
  merging: "合并中",
  completed: "已完成",
  cancelled: "已取消",
  failed: "失败",
};

export const STATUS_COLOR: Record<TaskStatus, string> = {
  pending: "#8b949e",
  connecting: "#58a6ff",
  downloading: "#3fb950",
  paused: "#d29922",
  merging: "#a371f7",
  completed: "#2ea043",
  cancelled: "#6e7681",
  failed: "#f85149",
};

/** 字节格式化 */
export function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const v = bytes / Math.pow(1024, i);
  return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${units[i]}`;
}

/** 速度格式化 */
export function formatSpeed(bytesPerSec: number): string {
  if (!bytesPerSec) return "—";
  return `${formatBytes(bytesPerSec)}/s`;
}

/** 时间格式化 */
export function formatEta(seconds: number | null): string {
  if (seconds === null || seconds < 0) return "—";
  if (seconds < 60) return `${seconds} 秒`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} 分 ${seconds % 60} 秒`;
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h} 小时 ${m} 分`;
}
