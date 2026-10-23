import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AppInfo,
  DownloadTask,
  NewTaskRequest,
  ProbeInfo,
  ProgressEvent,
  Settings,
} from "./types";

/** 后端事件名（与 Rust 保持一致） */
export const EVT_PROGRESS = "download://progress";
export const EVT_TASKS = "download://tasks";
/** 托盘「新建下载」 */
export const EVT_NEW_TASK = "tdm://new-task";
export const EVT_QUICK_OPEN = "tdm://quick-open";

export interface QuickOpenContext {
  mode: "manual" | "clipboard" | "browser" | "tasks";
  url: string | null;
  filename: string | null;
  request_id?: string | null;
}

export interface NotificationStatus {
  authorization: "not_determined" | "denied" | "authorized" | "unknown";
  alerts_enabled: boolean | null;
  center_enabled: boolean | null;
  bundle_id: string;
  delivered_ids: string[];
  last_error: string | null;
}

export interface BrowserExtensionStatus {
  chrome_version: string;
  chrome_installed_version: string | null;
  bridge_version: string;
  bridge_installed_version: string | null;
  safari_version: string;
  chrome_extension_id: string | null;
  chrome_directory: string;
}

export const api = {
  revealTaskFile: (id: string) => invoke<void>("reveal_task_file", { id }),
  saveTaskAs: (id: string) => invoke<string | null>("save_task_as", { id }),
  quickTaskInteraction: (active: boolean) => invoke<void>("quick_task_interaction", { active }),
  browserDialogContext: () => invoke<{ id: number; payload: QuickOpenContext } | null>("browser_dialog_context"),
  finishBrowserDialog: (id: number, accepted: boolean) => invoke<void>("finish_browser_dialog", { id, accepted }),
  quickWindowReady: () => invoke<void>("quick_window_ready"),
  browserDecision: (requestId: string, accepted: boolean) => invoke<void>("browser_download_decision", { requestId, accepted }),
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

  /** URL 预解析：文件名 / 大小 / 是否可续传 */
  probeUrl: (url: string, userAgent?: string, headers?: [string, string][]) =>
    invoke<ProbeInfo>("probe_url", { url, userAgent, headers }),

  pauseAll: () => invoke<void>("pause_all"),

  resumeAll: () => invoke<void>("resume_all"),

  /** 任务栏进度条：0.0~1.0，null 清除 */
  setProgressBar: (progress: number | null) =>
    invoke<void>("set_progress_bar", { progress }),

  showMainWindow: () => invoke<void>("show_main_window"),

  hideMainWindow: () => invoke<void>("hide_main_window"),

  quitApp: () => invoke<void>("quit_app"),

  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),

  getAutostart: () => invoke<boolean>("get_autostart"),

  getAppInfo: () => invoke<AppInfo>("get_app_info"),

  copyText: (text: string) => invoke<void>("copy_text", { text }),

  quickClipboardHint: () => invoke<string | null>("quick_clipboard_hint"),
  quickOpenContext: () => invoke<QuickOpenContext>("quick_open_context"),
  hideQuickWindow: () => invoke<void>("hide_quick_window"),
  showQuickInput: () => invoke<void>("show_quick_input"),
  notificationStatus: () => invoke<NotificationStatus>("notification_status"),
  testDownloadNotification: () => invoke<string>("test_download_notification"),
  browserExtensionStatus: () => invoke<BrowserExtensionStatus>("browser_extension_status"),
  prepareChromeExtension: () => invoke<BrowserExtensionStatus>("prepare_chrome_extension"),
  configureChromeExtension: (extensionId: string) => invoke<BrowserExtensionStatus>("configure_chrome_extension", { extensionId }),
  openChromeExtensionDirectory: () => invoke<void>("open_chrome_extension_directory"),
  openSafariConnector: () => invoke<void>("open_safari_connector"),
  openNotificationSettings: () => invoke<void>("open_notification_settings"),
};

/** 订阅实时进度事件 */
export function onProgress(cb: (e: ProgressEvent) => void) {
  return listen<ProgressEvent>(EVT_PROGRESS, (e) => cb(e.payload));
}

/** 订阅任务列表变化 */
export function onTasks(cb: (tasks: DownloadTask[]) => void) {
  return listen<DownloadTask[]>(EVT_TASKS, (e) => cb(e.payload));
}

/** 订阅托盘「新建下载」 */
export function onNewTask(cb: () => void) {
  return listen(EVT_NEW_TASK, () => cb());
}

export function onShowDownloads(cb: () => void) {
  return listen("tdm://show-downloads", () => cb());
}
