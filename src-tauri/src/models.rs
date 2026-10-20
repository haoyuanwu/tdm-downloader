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
pub struct NewTaskRequest {
    pub url: String,
    /// 保存目录
    pub dir: String,
    /// 文件名，为空则自动推断
    pub filename: Option<String>,
    /// 连接数，默认 8
    pub connections: Option<usize>,
    /// 限速 字节/秒，0 不限
    pub speed_limit: Option<u64>,
}

/// 应用设置
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl Default for Settings {
    fn default() -> Self {
        let dir = dirs_default_download();
        Settings {
            download_dir: dir,
            connections: 8,
            speed_limit: 0,
            max_concurrent: 3,
            enable_segment: true,
            segment_threshold: 1024 * 1024, // 1MB
        }
    }
}

/// 默认下载目录：用户 Downloads 文件夹
fn dirs_default_download() -> String {
    std::env::var("USERPROFILE")
        .map(|p| PathBuf::from(p).join("Downloads"))
        .or_else(|_| std::env::var("HOME").map(|p| PathBuf::from(p).join("Downloads")))
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string())
}
