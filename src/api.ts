import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DownloadTask, NewTaskRequest, ProgressEvent, Settings } from "./types";

/** 后端事件名（与 Rust engine.rs 保持一致） */
export const EVT_PROGRESS = "download://progress";
export const EVT_TASKS = "download://tasks";

export const api = {
  startDownload: (req: NewTaskRequest) =>
    invoke<DownloadTask>("start_download", { req }),

  listTasks: () => invoke<DownloadTask[]>("list_tasks"),

  pauseTask: (id: string) => invoke<void>("pause_task", { id }),

  resumeTask: (id: string) => invoke<void>("resume_task", { id }),

  cancelTask: (id: string) => invoke<void>("cancel_task", { id }),

  removeTask: (id: string, deleteFile = false) =>
    invoke<void>("remove_task", { id, deleteFile }),

  clearCompleted: () => invoke<void>("clear_completed"),

  getSettings: () => invoke<Settings>("get_settings"),

  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),

  openDownloadDir: (path: string) => invoke<void>("open_download_dir", { path }),

  pickDirectory: () => invoke<string | null>("pick_directory"),

  openPath: (path: string) => invoke<void>("open_path", { path }),
};

/** 订阅实时进度事件 */
export function onProgress(cb: (e: ProgressEvent) => void) {
  return listen<ProgressEvent>(EVT_PROGRESS, (e) => cb(e.payload));
}

/** 订阅任务列表变化 */
export function onTasks(cb: (tasks: DownloadTask[]) => void) {
  return listen<DownloadTask[]>(EVT_TASKS, (e) => cb(e.payload));
}
