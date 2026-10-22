//! 下载任务数据模型

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 任务状态机
/// Pending -> Downloading -> (Paused -> Downloading) -> Merging -> Completed
/// 任意阶段 -> Cancelled / Failed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    /// 等待调度
    Pending,
    /// 正在连接服务器探测资源
    Connecting,
    /// 正在下载
    Downloading,
    /// 已暂停（可续传）
    Paused,
    /// 所有分片完成，正在合并
    Merging,
    /// 已完成
    Completed,
    /// 已取消
    Cancelled,
    /// 失败
    Failed,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Connecting => "connecting",
            TaskStatus::Downloading => "downloading",
            TaskStatus::Paused => "paused",
            TaskStatus::Merging => "merging",
            TaskStatus::Completed => "completed",
            TaskStatus::Cancelled => "cancelled",
            TaskStatus::Failed => "failed",
        }
    }

    /// 是否处于"活跃"状态（占用连接资源）
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            TaskStatus::Connecting | TaskStatus::Downloading | TaskStatus::Merging
        )
    }

    /// 是否已终结（不会再变化）
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Failed
        )
    }
}

/// 单个分片的信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentProgress {
    pub index: usize,
    /// 分片起始字节（含）
    pub start: u64,
    /// 分片结束字节（含）
    pub end: u64,
    /// 已下载字节数
    pub downloaded: u64,
    /// 该分片是否完成
    pub done: bool,
}

impl SegmentProgress {
    pub fn total(&self) -> u64 {
        if self.end >= self.start {
            self.end - self.start + 1
        } else {
            0
        }
    }
}

/// 代理模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    /// 跟随系统代理
    System,
    /// 直连，不使用任何代理（含忽略环境变量）
    None,
    /// 自定义代理，proxy_addr 形如 http://127.0.0.1:8080 或 127.0.0.1:8080
    Custom,
}

impl Default for ProxyMode {
    fn default() -> Self {
        ProxyMode::System
    }
}

/// 目标文件已存在时的处理方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileExistsAction {
    /// 自动重命名：file.zip → file (1).zip
    Rename,
    /// 覆盖原文件
    Overwrite,
    /// 跳过（不再下载）
    Skip,
}

impl Default for FileExistsAction {
    fn default() -> Self {
        FileExistsAction::Rename
    }
}

/// 全部下载完成后的动作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AfterFinishAction {
    /// 什么都不做
    None,
    /// 退出程序
    Quit,
    /// 关机
    Shutdown,
    /// 休眠
    Sleep,
}

impl Default for AfterFinishAction {
    fn default() -> Self {
        AfterFinishAction::None
    }
}

/// 界面主题
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    System,
    Dark,
    Light,
}

impl Default for ThemeMode {
    fn default() -> Self {
        ThemeMode::Dark
    }
}

/// 下载任务（前后端通过 IPC 传输的快照）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: String,
    pub url: String,
    /// 最终保存路径（含文件名）
    pub save_path: String,
    pub filename: String,
    /// 文件总字节数，0 表示未知
    pub total_size: u64,
    /// 已下载字节数
    pub downloaded: u64,
    /// 当前状态
    pub status: TaskStatus,
    /// 分片进度
    pub segments: Vec<SegmentProgress>,
    /// 连接数（线程数）
    pub connections: usize,
    /// 单任务限速（字节/秒），0 表示不限速
    pub speed_limit: u64,
    /// 错误信息
    pub error: Option<String>,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 完成时间戳（毫秒）
    pub finished_at: Option<i64>,
    /// 该任务使用的 User-Agent（空则回落到全局设置）
    #[serde(default)]
    pub user_agent: String,
    /// 该任务附加的自定义请求头（如 Referer / Cookie）
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// 服务端是否支持断点续传（Accept-Ranges）
    #[serde(default)]
    pub resumable: bool,
}

impl DownloadTask {
    /// 下载进度百分比 0.0 ~ 100.0
    pub fn progress(&self) -> f64 {
        if self.total_size == 0 {
            return 0.0;
        }
        (self.downloaded as f64 / self.total_size as f64 * 100.0).min(100.0)
    }
}

/// URL 预解析结果（新建下载对话框中展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeInfo {
    /// 服务端建议的文件名（Content-Disposition），可能为空
    pub filename: String,
    /// 文件总字节数，0 表示未知
    pub total_size: u64,
    /// 是否支持断点续传（Accept-Ranges）
    pub accept_ranges: bool,
    /// 是否可连接
    pub ok: bool,
    /// 错误信息
    pub error: String,
}

/// 应用信息（关于页展示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInfo {
    pub version: String,
    pub exe_path: String,
}

/// 实时进度事件（高频推送给前端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    pub id: String,
    pub downloaded: u64,
    pub total_size: u64,
    /// 瞬时速度 字节/秒
    pub speed: u64,
    pub status: TaskStatus,
    /// 剩余时间估算（秒），未知为 None
    pub eta_secs: Option<u64>,
    pub segments: Vec<SegmentProgress>,
}

/// 新建下载任务的请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NewTaskRequest {
    pub url: String,
    /// 保存目录
    pub dir: String,
    /// 文件名，为空则自动推断
    pub filename: Option<String>,
    /// 连接数，默认取全局设置
    pub connections: Option<usize>,
    /// 限速 字节/秒，0 不限
    pub speed_limit: Option<u64>,
    /// 自定义 User-Agent
    pub user_agent: Option<String>,
    /// 附加请求头（Referer / Cookie / Authorization ...）
    pub headers: Option<Vec<(String, String)>>,
    /// 建任务后先暂停，等待手动开始
    pub start_paused: Option<bool>,
}

impl Default for NewTaskRequest {
    fn default() -> Self {
        NewTaskRequest {
            url: String::new(),
            dir: String::new(),
            filename: None,
            connections: None,
            speed_limit: None,
            user_agent: None,
            headers: None,
            start_paused: None,
        }
    }
}

/// 应用设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// 默认下载目录
    pub download_dir: String,
    /// 默认连接数
    pub connections: usize,
    /// 全局限速 字节/秒，0 不限
    pub speed_limit: u64,
    /// 最大并发任务数
    pub max_concurrent: usize,
    /// 是否启用分段下载（关闭则单连接）
    pub enable_segment: bool,
    /// 分段阈值：文件大于该字节数才分段
    pub segment_threshold: u64,
    /// 代理模式
    pub proxy_mode: ProxyMode,
    /// 自定义代理地址（proxy_mode = custom 时生效）
    pub proxy_addr: String,
    /// 默认 User-Agent
    pub user_agent: String,
    /// 文件已存在时的处理方式
    pub file_exists: FileExistsAction,
    /// 移除任务时是否询问本地文件处理方式
    pub confirm_remove_task: bool,
    /// 不询问时是否同时将本地文件移到废纸篓
    pub remove_task_files: bool,
    /// 全部下载完成后的动作
    pub after_finish: AfterFinishAction,
    /// 关闭窗口时最小化到托盘
    pub close_to_tray: bool,
    /// 开机自动启动
    pub autostart: bool,
    /// 界面主题
    pub theme: ThemeMode,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            download_dir: dirs_default_download(),
            connections: 8,
            speed_limit: 0,
            max_concurrent: 3,
            enable_segment: true,
            segment_threshold: 1024 * 1024, // 1MB
            proxy_mode: ProxyMode::System,
            proxy_addr: String::new(),
            user_agent: engine_default_ua(),
            file_exists: FileExistsAction::Rename,
            confirm_remove_task: true,
            remove_task_files: false,
            after_finish: AfterFinishAction::None,
            close_to_tray: true,
            autostart: false,
            theme: ThemeMode::Dark,
        }
    }
}

/// 与 engine::DEFAULT_UA 保持一致（放在这里避免 models 依赖 engine）
fn engine_default_ua() -> String {
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15 TDM-Downloader/0.2".to_string()
}

/// 默认下载目录：用户 Downloads 文件夹
fn dirs_default_download() -> String {
    std::env::var("USERPROFILE")
        .map(|p| PathBuf::from(p).join("Downloads"))
        .or_else(|_| std::env::var("HOME").map(|p| PathBuf::from(p).join("Downloads")))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string())
}
