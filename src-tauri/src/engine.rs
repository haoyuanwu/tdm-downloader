//! 多线程下载引擎
//!
//! 能力：
//! - 分段下载（HTTP Range，多连接并行）
//! - 断点续传（`.partN` 分片 + `.tdmstate.json` 状态持久化）
//! - 暂停 / 恢复 / 取消
//! - 限速（时间窗口配额）
//! - 分片合并

use crate::models::{
    AfterFinishAction, DownloadTask, FileExistsAction, NewTaskRequest, ProgressEvent, ProxyMode,
    SegmentProgress, Settings, TaskStatus,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use tokio::sync::{Mutex, Notify, Semaphore};
use tokio::task::JoinHandle;

pub const DEFAULT_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15 TDM-Downloader/0.2";

/// 进度事件名（前端订阅）
pub const EVT_PROGRESS: &str = "download://progress";
/// 任务列表变化事件名
pub const EVT_TASKS: &str = "download://tasks";

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ────────────────────────── HTTP 客户端 / 代理 ──────────────────────────
//
// 为什么需要显式处理代理：
// reqwest 默认会优先读取 HTTP_PROXY / HTTPS_PROXY 环境变量，但在 Windows 上
// 真实可用的代理通常写在「Internet 选项 → 局域网设置」里（注册表
// HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings）。
// 两者不一致时（例如环境变量指向一个不可达/受限的本地代理），下载会直接失败。
// 因此这里优先使用 Windows 系统代理，并尊重 ProxyOverride 绕过列表。
//
// 逃生阀：设置 TDM_USE_ENV_PROXY=1 可强制回到「由环境变量决定」的默认行为。

/// 读取 Windows 系统代理，返回 (代理地址, 绕过列表)
#[cfg(windows)]
fn windows_system_proxy() -> Option<(String, Option<String>)> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;

    // ProxyEnable 可能是 REG_DWORD(4) 或 REG_SZ(1)，两种都兼容
    let enabled: u32 = match key.get_value::<u32, _>("ProxyEnable") {
        Ok(v) => v,
        Err(_) => key
            .get_value::<String, _>("ProxyEnable")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0),
    };
    if enabled != 1 {
        return None;
    }

    let server: String = key.get_value("ProxyServer").ok()?;
    if server.trim().is_empty() {
        return None;
    }
    let bypass: Option<String> = key.get_value("ProxyOverride").ok();
    Some((server, bypass))
}

/// Read the effective macOS network proxy settings for the current location.
#[cfg(target_os = "macos")]
fn macos_system_proxy() -> Option<(String, Option<String>)> {
    let output = std::process::Command::new("/usr/sbin/scutil")
        .arg("--proxy")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let lines = String::from_utf8(output.stdout).ok()?;
    let value = |key: &str| -> Option<String> {
        lines.lines().find_map(|line| {
            let (name, val) = line.trim().split_once(" : ")?;
            (name == key).then(|| val.trim().to_string())
        })
    };
    let prefix = if value("HTTPSEnable").as_deref() == Some("1") {
        "HTTPS"
    } else if value("HTTPEnable").as_deref() == Some("1") {
        "HTTP"
    } else {
        return None;
    };
    let host = value(&format!("{prefix}Proxy"))?;
    let port = value(&format!("{prefix}Port"))?;
    if host.is_empty() || port.parse::<u16>().is_err() {
        return None;
    }
    let bypass = lines
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (index, val) = line.split_once(" : ")?;
            (index.parse::<usize>().is_ok()).then_some(val.trim().to_string())
        })
        .collect::<Vec<_>>()
        .join(",");
    Some((
        format!("http://{host}:{port}"),
        (!bypass.is_empty()).then_some(bypass),
    ))
}

/// 把形如 "127.0.0.1:6864" / "http=a:1;https=b:2" / "http://a:1" 归一化为带 scheme 的代理 URL
fn normalize_proxy_addr(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    // 分号分隔的多协议写法：http=...;https=...;ftp=...
    if s.contains('=') {
        let mut http_v: Option<String> = None;
        let mut https_v: Option<String> = None;
        for part in s.split([';', ',']) {
            let mut it = part.splitn(2, '=');
            let k = it.next().unwrap_or("").trim().to_lowercase();
            let v = it.next().unwrap_or("").trim().to_string();
            if v.is_empty() {
                continue;
            }
            match k.as_str() {
                "https" => https_v = Some(v),
                "http" | "ftp" | "socks" => {
                    if http_v.is_none() {
                        http_v = Some(v);
                    }
                }
                _ => {}
            }
        }
        return https_v.or(http_v).map(|v| with_scheme(&v));
    }
    Some(with_scheme(s))
}

fn with_scheme(v: &str) -> String {
    if v.contains("://") {
        v.to_string()
    } else {
        format!("http://{}", v)
    }
}

/// 构造 HTTP 客户端所需的配置
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub proxy_mode: ProxyMode,
    pub proxy_addr: String,
    pub user_agent: String,
    pub headers: Vec<(String, String)>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig {
            proxy_mode: ProxyMode::System,
            proxy_addr: String::new(),
            user_agent: DEFAULT_UA.to_string(),
            headers: Vec::new(),
        }
    }
}

/// 构造统一配置的 HTTP 客户端（默认跟随系统代理）
pub fn build_http_client() -> reqwest::Client {
    build_task_client(&ClientConfig::default())
}

/// 按配置构造客户端：支持代理模式、自定义 UA 与附加请求头
pub fn build_task_client(cfg: &ClientConfig) -> reqwest::Client {
    let ua = if cfg.user_agent.trim().is_empty() {
        DEFAULT_UA
    } else {
        cfg.user_agent.as_str()
    };

    let mut builder = reqwest::Client::builder()
        .user_agent(ua)
        .connect_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10));

    // 附加请求头（Referer / Cookie / Authorization 等）
    let mut hm = reqwest::header::HeaderMap::new();
    for (k, v) in cfg.headers.iter() {
        if k.trim().is_empty() {
            continue;
        }
        if let (Ok(name), Ok(val)) = (
            reqwest::header::HeaderName::from_bytes(k.trim().as_bytes()),
            reqwest::header::HeaderValue::from_str(v),
        ) {
            hm.insert(name, val);
        }
    }
    if !hm.is_empty() {
        builder = builder.default_headers(hm);
    }

    match cfg.proxy_mode {
        // 直连：忽略系统代理与环境变量
        ProxyMode::None => {
            builder = builder.no_proxy();
        }
        // 自定义代理
        ProxyMode::Custom => {
            if let Some(url) = normalize_proxy_addr(&cfg.proxy_addr) {
                if let Ok(p) = reqwest::Proxy::all(&url) {
                    builder = builder.proxy(p);
                }
            }
        }
        ProxyMode::System => {
            #[cfg(windows)]
            {
                // 逃生阀：TDM_USE_ENV_PROXY=1 时回到「由环境变量决定」的默认行为
                let use_env = std::env::var("TDM_USE_ENV_PROXY")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false);
                if !use_env {
                    match windows_system_proxy() {
                        Some((addr, bypass)) => {
                            if let Some(url) = normalize_proxy_addr(&addr) {
                                if let Ok(p) = reqwest::Proxy::all(&url) {
                                    let p = match bypass {
                                        Some(b) if !b.trim().is_empty() => {
                                            p.no_proxy(reqwest::NoProxy::from_string(
                                                &b.replace(';', ",").replace("<local>", ""),
                                            ))
                                        }
                                        _ => p,
                                    };
                                    builder = builder.proxy(p);
                                }
                            }
                        }
                        None => {
                            // 系统未启用代理：忽略环境变量，避免被无效的 env 代理劫持
                            builder = builder.no_proxy();
                        }
                    }
                }
            }
            #[cfg(target_os = "macos")]
            {
                if let Some((addr, bypass)) = macos_system_proxy() {
                    if let Ok(p) = reqwest::Proxy::all(&addr) {
                        let p = match bypass {
                            Some(b) => p.no_proxy(reqwest::NoProxy::from_string(&b)),
                            None => p,
                        };
                        builder = builder.proxy(p);
                    }
                } else {
                    builder = builder.no_proxy();
                }
            }
            #[cfg(not(any(windows, target_os = "macos")))]
            {
                // Other platforms use reqwest's environment defaults.
            }
        }
    }

    builder.build().unwrap_or_else(|_| reqwest::Client::new())
}

// ────────────────────────── 限速器 ──────────────────────────

/// 令牌桶限速器。limit == 0 表示不限速。
///
/// 桶容量 = 1 秒的配额，但初始为空桶，因此平均速率不会超过 limit 字节/秒
/// （不做首次突发，避免短任务看起来没被限速）。
/// 相比早期的「1 秒固定窗口」实现，令牌桶不会因为窗口滚动而白送配额。
pub struct Limiter {
    limit: AtomicU64,
    /// (上次补充令牌的时刻, 当前令牌数，可为负表示需要等待)
    bucket: Mutex<(Instant, f64)>,
}

impl Limiter {
    pub fn new(limit: u64) -> Self {
        Limiter {
            limit: AtomicU64::new(limit),
            // 初始空桶：严格限速，平均速率 <= limit
            bucket: Mutex::new((Instant::now(), 0.0)),
        }
    }

    pub fn set_limit(&self, limit: u64) {
        self.limit.store(limit, Ordering::SeqCst);
    }

    /// 消费 n 字节配额，令牌不足则休眠到补齐为止
    pub async fn take(&self, n: u64) {
        let limit = self.limit.load(Ordering::SeqCst);
        if limit == 0 || n == 0 {
            return;
        }
        let rate = limit as f64; // 字节/秒
        let mut b = self.bucket.lock().await;
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(b.0).as_secs_f64();
        b.0 = now;
        // 按流逝时间补充令牌，桶容量上限为 1 秒的配额
        b.1 = (b.1 + elapsed * rate).min(rate);
        b.1 -= n as f64;
        if b.1 < 0.0 {
            // 欠账：休眠到令牌回正（这段时间会在下次 take 时自动补回）
            let wait = Duration::from_secs_f64((-b.1) / rate);
            drop(b);
            tokio::time::sleep(wait).await;
        }
    }
}

// ────────────────────────── 任务句柄 ──────────────────────────

/// 一个任务的运行时控制句柄
#[derive(Clone)]
pub struct TaskHandle {
    pub task: Arc<Mutex<DownloadTask>>,
    /// 请求取消
    pub cancel: Arc<AtomicBool>,
    /// 请求暂停
    pub pause: Arc<AtomicBool>,
    /// 当前速度 B/s
    pub speed: Arc<AtomicU64>,
    /// 限速器
    pub limiter: Arc<Limiter>,
    /// 运行中的 join handle（用于等待结束）
    pub worker: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// 移除任务会中止整个下载流程，等待所有写入结束后处理本地文件。
    pub removed: Arc<Notify>,
}

impl TaskHandle {
    pub fn new(task: DownloadTask) -> Self {
        let speed_limit = task.speed_limit;
        TaskHandle {
            task: Arc::new(Mutex::new(task)),
            cancel: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
            speed: Arc::new(AtomicU64::new(0)),
            limiter: Arc::new(Limiter::new(speed_limit)),
            worker: Arc::new(Mutex::new(None)),
            removed: Arc::new(Notify::new()),
        }
    }

    /// 汇总已下载字节数
    pub async fn downloaded(&self) -> u64 {
        let t = self.task.lock().await;
        t.segments.iter().map(|s| s.downloaded).sum()
    }
}

// ────────────────────────── 引擎 ──────────────────────────

pub struct DownloadEngine {
    pub client: reqwest::Client,
    pub tasks: Arc<Mutex<HashMap<String, TaskHandle>>>,
    pub settings: Arc<Mutex<Settings>>,
    /// 全局并发任务数限制
    pub slots: Arc<Semaphore>,
}

impl DownloadEngine {
    pub fn new() -> Self {
        let client = build_http_client();
        let settings = load_settings().unwrap_or_default();
        let max = settings.max_concurrent.max(1);
        DownloadEngine {
            client,
            tasks: Arc::new(Mutex::new(HashMap::new())),
            settings: Arc::new(Mutex::new(settings)),
            slots: Arc::new(Semaphore::new(max)),
        }
    }

    /// 生成任务快照列表
    pub async fn snapshot(&self) -> Vec<DownloadTask> {
        let map = self.tasks.lock().await;
        let mut out = Vec::with_capacity(map.len());
        for h in map.values() {
            let mut t = h.task.lock().await.clone();
            t.downloaded = t.segments.iter().map(|s| s.downloaded).sum();
            out.push(t);
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        out
    }

    /// 取单个任务快照
    pub async fn get(&self, id: &str) -> Option<DownloadTask> {
        let map = self.tasks.lock().await;
        let h = map.get(id)?;
        let mut t = h.task.lock().await.clone();
        t.downloaded = t.segments.iter().map(|s| s.downloaded).sum();
        Some(t)
    }
}

// ────────────────────────── 状态持久化 ──────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct PersistedState {
    url: String,
    save_path: String,
    total_size: u64,
    connections: usize,
    speed_limit: u64,
    segments: Vec<SegmentProgress>,
}

fn state_path(save_path: &str) -> PathBuf {
    PathBuf::from(format!("{}.tdmstate.json", save_path))
}

pub fn part_path(save_path: &str, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.part{}", save_path, index))
}

fn save_state(task: &DownloadTask) {
    let st = PersistedState {
        url: task.url.clone(),
        save_path: task.save_path.clone(),
        total_size: task.total_size,
        connections: task.connections,
        speed_limit: task.speed_limit,
        segments: task.segments.clone(),
    };
    if let Ok(json) = serde_json::to_string(&st) {
        let _ = fs::write(state_path(&task.save_path), json);
    }
}

fn load_state(save_path: &str) -> Option<PersistedState> {
    let p = state_path(save_path);
    let data = fs::read_to_string(p).ok()?;
    serde_json::from_str::<PersistedState>(&data).ok()
}

// ────────────────────────── 设置持久化 ──────────────────────────

fn settings_file() -> PathBuf {
    #[cfg(target_os = "macos")]
    let base = std::env::var("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support"))
        .unwrap_or_else(|_| PathBuf::from("."));
    #[cfg(not(target_os = "macos"))]
    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let dir = base.join("tdm-downloader");
    let _ = fs::create_dir_all(&dir);
    dir.join("settings.json")
}

pub fn load_settings() -> Option<Settings> {
    let data = fs::read_to_string(settings_file()).ok()?;
    serde_json::from_str::<Settings>(&data).ok()
}

pub fn persist_settings(s: &Settings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(s).map_err(|error| error.to_string())?;
    fs::write(settings_file(), json).map_err(|error| format!("保存设置失败：{error}"))
}

// ────────────────────────── 文件名推断 ──────────────────────────

/// 从 URL 推断文件名
pub fn filename_from_url(url: &str) -> String {
    let cleaned = url.split('?').next().unwrap_or(url);
    let cleaned = cleaned.split('#').next().unwrap_or(cleaned);
    let seg = cleaned
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    let decoded = percent_decode(seg);
    let name = decoded.trim().to_string();
    if name.is_empty() || name.contains("..") {
        format!("download_{}", now_ms())
    } else {
        sanitize_filename(&name)
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// 去掉 Windows 非法文件名字符
pub fn sanitize_filename(name: &str) -> String {
    let bad = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let s: String = name.chars().filter(|c| !bad.contains(c)).collect();
    let s = s.trim().to_string();
    if s.is_empty() {
        format!("download_{}", now_ms())
    } else {
        s
    }
}

/// 从 Content-Disposition 头解析文件名
fn filename_from_cd(header: &str) -> Option<String> {
    // 优先 filename*=UTF-8''xxx
    if let Some(idx) = header.find("filename*=") {
        let rest = &header[idx + 10..];
        let rest = rest.split(';').next().unwrap_or(rest).trim();
        if let Some(pos) = rest.find("''") {
            let val = &rest[pos + 2..];
            let val = val.trim_matches('\'').trim_matches('"');
            let dec = percent_decode(val);
            if !dec.is_empty() {
                return Some(sanitize_filename(&dec));
            }
        }
    }
    if let Some(idx) = header.find("filename=") {
        let rest = &header[idx + 9..];
        let rest = rest.split(';').next().unwrap_or(rest).trim();
        let val = rest.trim_matches('"').trim_matches('\'');
        if !val.is_empty() {
            return Some(sanitize_filename(val));
        }
    }
    None
}

/// 避免覆盖已存在文件：a.txt -> a (1).txt
fn unique_path(dir: &Path, filename: &str) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| filename.to_string());
    let ext = Path::new(filename)
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    for i in 1..1000 {
        let p = dir.join(format!("{} ({}).{}", stem, i_i_zero(i), ext));
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!("{}_{}{}", stem, now_ms(), ext))
}

fn i_i_zero(i: usize) -> String {
    i.to_string()
}

// ────────────────────────── 资源探测 ──────────────────────────

pub struct ProbeResult {
    pub total_size: u64,
    pub accept_ranges: bool,
    pub filename: Option<String>,
}

/// HEAD 探测资源信息；HEAD 失败则退化为 GET（只读头部后丢弃 body）
pub async fn probe(client: &reqwest::Client, url: &str) -> Result<ProbeResult, String> {
    let head = client.head(url).send().await;
    match head {
        Ok(resp) if resp.status().is_success() || resp.status() == 405 => {
            let total_size = resp
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let accept_ranges = resp
                .headers()
                .get(reqwest::header::ACCEPT_RANGES)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_lowercase().contains("bytes"))
                .unwrap_or(false);
            let filename = resp
                .headers()
                .get(reqwest::header::CONTENT_DISPOSITION)
                .and_then(|v| v.to_str().ok())
                .and_then(filename_from_cd);
            return Ok(ProbeResult {
                total_size,
                accept_ranges,
                filename,
            });
        }
        _ => {}
    }

    // 退化：发 GET 取头部
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    let total_size = resp.content_length().unwrap_or(0);
    let accept_ranges = resp
        .headers()
        .get(reqwest::header::ACCEPT_RANGES)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_lowercase().contains("bytes"))
        .unwrap_or(false);
    let filename = resp
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(filename_from_cd);
    Ok(ProbeResult {
        total_size,
        accept_ranges,
        filename,
    })
}

// ────────────────────────── 分片下载 ──────────────────────────

/// 单个 chunk 的等待上限（秒）。超过即判定连接停滞，断开重连。
const STALL_TIMEOUT_SECS: u64 = 20;
/// 单个分片的最大重试次数
const SEGMENT_MAX_RETRIES: usize = 8;

/// 单次分片下载尝试的结果
pub enum SegOutcome {
    /// 分片已完整下载
    Done,
    /// 被用户暂停 / 取消（不再重试）
    Stopped,
    /// 出错或停滞（可重试）
    Error(String),
}

/// 单个分片的下载循环（带停滞超时 + 指数退避重连）
///
/// 为什么要这套机制：
/// CDN / 代理通常只放行有限并发，多出来的连接会被**挂起**——既不给数据也不报错。
/// 缺乏超时保护时这些分片会永久等待，任务就卡在某个百分比不动（实测 16 连接下
/// 13 个分片被挂起，8 分钟零进度）。这里对每个 chunk 的等待设上限，
/// 一旦停滞就从已下载位置重新发起 Range 请求续传。
pub async fn download_segment(
    client: reqwest::Client,
    url: String,
    part: PathBuf,
    seg_index: usize,
    start: u64,
    end: u64,
    unbounded: bool,
    task: Arc<Mutex<DownloadTask>>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    limiter: Arc<Limiter>,
) -> Result<(), String> {
    let mut attempt = 0usize;
    let mut last_err = String::new();

    loop {
        if cancel.load(Ordering::SeqCst) || pause.load(Ordering::SeqCst) {
            return if last_err.is_empty() {
                Ok(())
            } else {
                Err(last_err)
            };
        }

        match download_segment_once(
            &client, &url, &part, seg_index, start, end, unbounded, &task, &cancel, &pause,
            &limiter,
        )
        .await
        {
            SegOutcome::Done | SegOutcome::Stopped => return Ok(()),
            SegOutcome::Error(e) => {
                if cancel.load(Ordering::SeqCst) || pause.load(Ordering::SeqCst) {
                    return Ok(());
                }
                last_err = e;
                attempt += 1;
                if attempt > SEGMENT_MAX_RETRIES {
                    return Err(format!(
                        "分片 {} 重试 {} 次仍失败: {}",
                        seg_index, SEGMENT_MAX_RETRIES, last_err
                    ));
                }
                // 指数退避：1, 2, 4, 8, 15, 15 ... 秒
                let backoff = std::cmp::min(1u64 << (attempt - 1).min(4), 15);
                tokio::time::sleep(Duration::from_secs(backoff)).await;
            }
        }
    }
}

/// 单次尝试：发起 Range 请求并写入分片文件，遇到停滞立即返回错误
async fn download_segment_once(
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    seg_index: usize,
    start: u64,
    end: u64,
    unbounded: bool,
    task: &Arc<Mutex<DownloadTask>>,
    cancel: &Arc<AtomicBool>,
    pause: &Arc<AtomicBool>,
    limiter: &Arc<Limiter>,
) -> SegOutcome {
    let downloaded: u64 = if part.exists() {
        fs::metadata(part).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    let total = if unbounded {
        u64::MAX
    } else if end >= start {
        end - start + 1
    } else {
        0
    };

    if !unbounded && downloaded >= total && total > 0 {
        let mut t = task.lock().await;
        if let Some(s) = t.segments.get_mut(seg_index) {
            s.downloaded = total;
            s.done = true;
        }
        return SegOutcome::Done;
    }

    let mut file = match OpenOptions::new().create(true).append(true).open(part) {
        Ok(f) => f,
        Err(e) => {
            return SegOutcome::Error(format!("无法创建分片文件 {}: {}", part.display(), e));
        }
    };

    // 续传位置
    let range_start = start + downloaded;
    let mut req = client.get(url);
    if !unbounded {
        req = req.header(
            reqwest::header::RANGE,
            format!("bytes={}-{}", range_start, end),
        );
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return SegOutcome::Error(format!("分片 {} 请求失败: {}", seg_index, e));
        }
    };

    let status = resp.status();
    // 429 / 503：服务端限流，退避后重试
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
    {
        return SegOutcome::Error(format!("分片 {} 被限流 HTTP {}", seg_index, status));
    }
    let status_ok = status == reqwest::StatusCode::PARTIAL_CONTENT || status.is_success();
    if !status_ok {
        return SegOutcome::Error(format!("分片 {} HTTP {}", seg_index, status));
    }

    let mut stream = resp.bytes_stream();
    let mut got = downloaded;

    loop {
        if cancel.load(Ordering::SeqCst) || pause.load(Ordering::SeqCst) {
            let _ = file.flush();
            return SegOutcome::Stopped;
        }

        // 停滞检测：等待下一个 chunk 超过阈值即判定连接挂起
        match tokio::time::timeout(Duration::from_secs(STALL_TIMEOUT_SECS), stream.next()).await {
            Ok(Some(chunk)) => {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        return SegOutcome::Error(format!("分片 {} 读取失败: {}", seg_index, e));
                    }
                };
                let n = chunk.len() as u64;
                limiter.take(n).await;
                if let Err(e) = file.write_all(&chunk) {
                    return SegOutcome::Error(format!("分片 {} 写入失败: {}", seg_index, e));
                }
                got += n;
                let mut t = task.lock().await;
                if let Some(s) = t.segments.get_mut(seg_index) {
                    s.downloaded = got;
                    if !unbounded && got >= s.total() {
                        s.done = true;
                    }
                }
                if unbounded {
                    t.total_size = t.segments.iter().map(|x| x.downloaded).sum();
                }
            }
            Ok(None) => break, // 流正常结束
            Err(_) => {
                let _ = file.flush();
                // 已下载部分落盘，外层会从该位置续传重试
                let mut t = task.lock().await;
                if let Some(s) = t.segments.get_mut(seg_index) {
                    s.downloaded = got;
                }
                return SegOutcome::Error(format!(
                    "分片 {} 停滞 {}s（已下载 {} 字节）",
                    seg_index, STALL_TIMEOUT_SECS, got
                ));
            }
        }
    }

    let _ = file.flush();

    let mut t = task.lock().await;
    if let Some(s) = t.segments.get_mut(seg_index) {
        s.downloaded = got;
        if !unbounded && got >= s.total() {
            s.done = true;
        }
    }

    // 流结束了但没下载够：连接被提前切断，需要重连续传
    if !unbounded && got < total {
        return SegOutcome::Error(format!(
            "分片 {} 连接提前断开（{}/{} 字节）",
            seg_index, got, total
        ));
    }
    SegOutcome::Done
}

// ────────────────────────── 任务主流程 ──────────────────────────

/// 构建分片（新建或按已有状态续传）
pub fn build_segments(
    total_size: u64,
    connections: usize,
    prev: &[SegmentProgress],
) -> Vec<SegmentProgress> {
    if total_size == 0 {
        return vec![SegmentProgress {
            index: 0,
            start: 0,
            end: u64::MAX,
            downloaded: 0,
            done: false,
        }];
    }
    let n = connections.max(1).min(total_size as usize);
    if n == 1 {
        return vec![SegmentProgress {
            index: 0,
            start: 0,
            end: total_size - 1,
            downloaded: 0,
            done: false,
        }];
    }
    let chunk = total_size / n as u64;
    let mut segs = Vec::with_capacity(n);
    let mut start = 0u64;
    for i in 0..n {
        let end = if i == n - 1 {
            total_size - 1
        } else {
            start + chunk - 1
        };
        let downloaded = prev
            .iter()
            .find(|p| p.index == i && p.start == start && p.end == end)
            .map(|p| p.downloaded.min(end - start + 1))
            .unwrap_or(0);
        segs.push(SegmentProgress {
            index: i,
            start,
            end,
            downloaded,
            done: downloaded >= end - start + 1,
        });
        start = end + 1;
    }
    segs
}

/// 合并所有分片到最终文件
pub fn merge_parts(save_path: &str, count: usize) -> Result<(), String> {
    let final_path = PathBuf::from(save_path);
    if let Some(parent) = final_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut out = File::create(&final_path)
        .map_err(|e| format!("无法创建目标文件 {}: {}", final_path.display(), e))?;
    for i in 0..count {
        let p = part_path(save_path, i);
        if p.exists() {
            let data = fs::read(&p).map_err(|e| format!("读取分片失败: {}", e))?;
            out.write_all(&data)
                .map_err(|e| format!("写入合并失败: {}", e))?;
        }
    }
    out.flush().map_err(|e| format!("flush 失败: {}", e))?;
    drop(out);
    // 清理分片与状态
    for i in 0..count {
        let _ = fs::remove_file(part_path(save_path, i));
    }
    let _ = fs::remove_file(state_path(save_path));
    Ok(())
}

/// 任务主循环（一次"运行"：从当前进度跑到 暂停/完成/取消/失败）
pub async fn run_task(app: tauri::AppHandle, engine: Arc<DownloadEngine>, handle: TaskHandle) {
    let id = {
        let t = handle.task.lock().await;
        t.id.clone()
    };

    // 并发槽位（受 max_concurrent 限制）
    let _permit = match engine.slots.clone().acquire_owned().await {
        Ok(p) => p,
        Err(_) => return,
    };

    // 速度监控任务
    let mon_cancel = Arc::new(AtomicBool::new(false));
    let mon = {
        let task = handle.task.clone();
        let speed = handle.speed.clone();
        let app_c = app.clone();
        let cancel = mon_cancel.clone();
        let task_cancel = handle.cancel.clone();
        let id_c = id.clone();
        tokio::spawn(async move {
            let mut last: u64 = 0;
            let mut last_time = Instant::now();
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if cancel.load(Ordering::SeqCst) || task_cancel.load(Ordering::SeqCst) {
                    break;
                }
                let (downloaded, total, status, segs) = {
                    let t = task.lock().await;
                    let d: u64 = t.segments.iter().map(|s| s.downloaded).sum();
                    (d, t.total_size, t.status, t.segments.clone())
                };
                let now = Instant::now();
                let dt = now.duration_since(last_time).as_secs_f64();
                let sp = if dt > 0.0 {
                    ((downloaded.saturating_sub(last)) as f64 / dt) as u64
                } else {
                    0
                };
                last = downloaded;
                last_time = now;
                speed.store(sp, Ordering::SeqCst);

                let eta = if sp > 0 && total > downloaded && total != u64::MAX {
                    Some((total - downloaded) / sp)
                } else {
                    None
                };
                let evt = ProgressEvent {
                    id: id_c.clone(),
                    downloaded,
                    total_size: if total == u64::MAX { 0 } else { total },
                    speed: sp,
                    status,
                    eta_secs: eta,
                    segments: segs,
                };
                let _ = app_c.emit(EVT_PROGRESS, evt);
            }
        })
    };

    // 按任务配置构建独立 HTTP 客户端（代理模式 / UA / 自定义请求头）
    let task_client = {
        let (ua, headers) = {
            let t = handle.task.lock().await;
            (t.user_agent.clone(), t.headers.clone())
        };
        let settings = engine.settings.lock().await;
        let ua_final = if ua.trim().is_empty() {
            settings.user_agent.clone()
        } else {
            ua
        };
        let cfg = ClientConfig {
            proxy_mode: settings.proxy_mode,
            proxy_addr: settings.proxy_addr.clone(),
            user_agent: ua_final,
            headers,
        };
        drop(settings);
        build_task_client(&cfg)
    };

    // 1. 探测资源
    {
        let mut t = handle.task.lock().await;
        t.status = TaskStatus::Connecting;
    }
    let (url, _save_path, connections, _speed_limit) = {
        let t = handle.task.lock().await;
        (
            t.url.clone(),
            t.save_path.clone(),
            t.connections,
            t.speed_limit,
        )
    };

    let probe_res = probe(&task_client, &url).await;
    let (total_size, accept_ranges, server_filename) = match probe_res {
        Ok(p) => (p.total_size, p.accept_ranges, p.filename),
        Err(e) => {
            let mut t = handle.task.lock().await;
            t.status = TaskStatus::Failed;
            t.error = Some(format!("无法连接资源: {}", e));
            mon_cancel.store(true, Ordering::SeqCst);
            let _ = mon.await;
            let snapshot = t.clone();
            drop(t);
            notify(&app, &engine, snapshot).await;
            return;
        }
    };

    // 文件名：请求指定 > 服务端 Content-Disposition > URL 推断
    let skipped = {
        let mut t = handle.task.lock().await;
        if t.filename.is_empty() || t.filename == "download" {
            if let Some(f) = server_filename.clone() {
                t.filename = f;
            }
        }
        t.resumable = accept_ranges;
        let dir = Path::new(&t.save_path)
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let _ = fs::create_dir_all(&dir);
        let action = {
            let s = engine.settings.lock().await;
            s.file_exists
        };
        let direct = dir.join(&t.filename);
        let (full, skip) = match action {
            FileExistsAction::Overwrite => (direct, false),
            FileExistsAction::Skip => (direct.clone(), direct.exists()),
            FileExistsAction::Rename => (unique_path(&dir, &t.filename), false),
        };
        t.filename = full
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| t.filename.clone());
        t.save_path = full.to_string_lossy().to_string();
        t.total_size = total_size;
        if skip {
            t.status = TaskStatus::Completed;
            t.downloaded = total_size;
            t.finished_at = Some(now_ms());
            t.error = Some("文件已存在，按设置跳过".to_string());
        }
        skip
    };

    if skipped {
        mon_cancel.store(true, Ordering::SeqCst);
        let _ = mon.await;
        let snapshot = {
            let t = handle.task.lock().await;
            t.clone()
        };
        notify(&app, &engine, snapshot).await;
        maybe_run_after_finish(&app, &engine).await;
        return;
    }

    let (save_path, total_size, unbounded) = {
        let t = handle.task.lock().await;
        (t.save_path.clone(), t.total_size, t.total_size == 0)
    };

    // 2. 构建/恢复分片
    {
        let settings = engine.settings.lock().await;
        let mut t = handle.task.lock().await;
        let prev = load_state(&save_path)
            .map(|s| s.segments)
            .unwrap_or_default();
        let use_segment = settings.enable_segment
            && !unbounded
            && accept_ranges
            && total_size >= settings.segment_threshold;
        let conns = if use_segment { connections.max(1) } else { 1 };
        t.connections = conns;
        t.segments = build_segments(if unbounded { 0 } else { total_size }, conns, &prev);
    }

    {
        let mut t = handle.task.lock().await;
        t.status = TaskStatus::Downloading;
        t.error = None;
    }

    // 3. 并发下载各分片
    let segs_snapshot = {
        let t = handle.task.lock().await;
        t.segments.clone()
    };

    // 直接并发轮询分片，取消父流程时会一并释放分片请求和文件句柄。
    let mut futs = futures_util::stream::FuturesUnordered::new();
    for s in segs_snapshot.iter() {
        if s.done {
            continue;
        }
        let client = task_client.clone();
        let url_c = url.clone();
        let part = part_path(&save_path, s.index);
        let idx = s.index;
        let start = s.start;
        let end = s.end;
        let task_c = handle.task.clone();
        let cancel_c = handle.cancel.clone();
        let pause_c = handle.pause.clone();
        let limiter_c = handle.limiter.clone();
        let unbounded_c = unbounded;
        futs.push(async move {
            download_segment(
                client,
                url_c,
                part,
                idx,
                start,
                end,
                unbounded_c,
                task_c,
                cancel_c,
                pause_c,
                limiter_c,
            )
            .await
        });
    }

    let mut first_error: Option<String> = None;
    while let Some(result) = futs.next().await {
        match result {
            Ok(()) => {}
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }

    // 4. 判定结果
    let cancelled = handle.cancel.load(Ordering::SeqCst);
    let paused = handle.pause.load(Ordering::SeqCst);

    // 收尾前先释放锁，避免 guard 跨 await
    {
        let mut t = handle.task.lock().await;
        t.downloaded = t.segments.iter().map(|s| s.downloaded).sum();
        let all_done = !t.segments.is_empty() && t.segments.iter().all(|s| s.done);

        if cancelled {
            t.status = TaskStatus::Cancelled;
        } else if paused {
            t.status = TaskStatus::Paused;
            save_state(&t);
        } else if let Some(e) = first_error {
            // 部分失败：保存进度，允许后续恢复重试
            t.error = Some(e.clone());
            t.status = TaskStatus::Failed;
            save_state(&t);
        } else if all_done || unbounded {
            // 合并分片
            t.status = TaskStatus::Merging;
            let count = t.segments.len();
            let sp = t.save_path.clone();
            drop(t);
            let merge_res = merge_parts(&sp, count);
            let mut t = handle.task.lock().await;
            match merge_res {
                Ok(()) => {
                    t.status = TaskStatus::Completed;
                    t.downloaded = t.total_size.max(t.downloaded);
                    t.finished_at = Some(now_ms());
                }
                Err(e) => {
                    t.status = TaskStatus::Failed;
                    t.error = Some(e);
                }
            }
        } else {
            t.status = TaskStatus::Paused;
            save_state(&t);
        }
    }

    mon_cancel.store(true, Ordering::SeqCst);
    let _ = mon.await;

    let snapshot = {
        let t = handle.task.lock().await;
        t.clone()
    };
    notify(&app, &engine, snapshot).await;

    // 全部任务结束后，按设置执行收尾动作（退出 / 关机 / 休眠）
    maybe_run_after_finish(&app, &engine).await;
}

/// 推送单个任务更新 + 任务列表更新
async fn notify(app: &tauri::AppHandle, engine: &Arc<DownloadEngine>, task: DownloadTask) {
    let _ = app.emit(format!("download://task/{}", task.id).as_str(), &task);
    let list = engine.snapshot().await;
    let _ = app.emit(EVT_TASKS, list);
    // 只在实际下载并合并成功后通知；文件已存在而跳过的任务不提醒。
    if task.status == TaskStatus::Completed && task.error.is_none() {
        if let Err(error) = crate::notifications::completed(
            app,
            &format!("tdm.download.{}", task.id),
            "下载完成",
            &format!("{}\n已保存到 {}", task.filename, task.save_path),
        )
        .await
        {
            eprintln!("发送下载完成通知失败：{error}");
        }
    }
}

/// 所有任务都结束后，按设置执行收尾动作
///
/// 只要还有未终结的任务（进行中 / 暂停 / 等待）就不触发，避免误关机。
pub async fn maybe_run_after_finish(app: &tauri::AppHandle, engine: &Arc<DownloadEngine>) {
    let action = {
        let s = engine.settings.lock().await;
        s.after_finish
    };
    if action == AfterFinishAction::None {
        return;
    }
    let list = engine.snapshot().await;
    if list.is_empty() {
        return;
    }
    if list.iter().any(|t| !t.status.is_terminal()) {
        return;
    }

    match action {
        AfterFinishAction::Quit => app.exit(0),
        AfterFinishAction::Shutdown => {
            let _ = system_power(true);
        }
        AfterFinishAction::Sleep => {
            let _ = system_power(false);
        }
        AfterFinishAction::None => {}
    }
}

/// 执行系统关机 / 休眠
fn system_power(shutdown: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        if shutdown {
            use std::os::windows::process::CommandExt;
            std::process::Command::new("shutdown")
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .args([
                    "/s",
                    "/t",
                    "30",
                    "/c",
                    "TDM Downloader: all downloads finished.",
                ])
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string())
        } else {
            // SetSuspendState 并非 rundll32 入口，必须按原生函数签名调用。
            if unsafe { windows::Win32::System::Power::SetSuspendState(false, false, false) } {
                Ok(())
            } else {
                Err(format!("进入睡眠失败：{}", std::io::Error::last_os_error()))
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if shutdown {
            std::process::Command::new("osascript")
                .args([
                    "-e",
                    "delay 30",
                    "-e",
                    "tell application \"System Events\" to shut down",
                ])
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string())
        } else {
            std::process::Command::new("pmset")
                .arg("sleepnow")
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = shutdown;
        Err("当前平台不支持该操作".to_string())
    }
}

/// 新建任务并入队
pub async fn spawn_task(
    app: tauri::AppHandle,
    engine: Arc<DownloadEngine>,
    req: NewTaskRequest,
) -> Result<DownloadTask, String> {
    let settings = engine.settings.lock().await;
    let dir = if req.dir.is_empty() {
        settings.download_dir.clone()
    } else {
        req.dir.clone()
    };
    let connections = req.connections.unwrap_or(settings.connections);
    let speed_limit = req.speed_limit.unwrap_or(0);
    drop(settings);

    let dir_path = PathBuf::from(&dir);
    fs::create_dir_all(&dir_path).map_err(|e| format!("无法创建目录 {}: {}", dir, e))?;

    let filename = req
        .filename
        .clone()
        .filter(|f| !f.trim().is_empty())
        .unwrap_or_else(|| filename_from_url(&req.url));
    let filename = sanitize_filename(&filename);

    let id = uuid::Uuid::new_v4().to_string();
    let start_paused = req.start_paused.unwrap_or(false);
    let task = DownloadTask {
        id: id.clone(),
        url: req.url.clone(),
        save_path: dir_path.join(&filename).to_string_lossy().to_string(),
        filename,
        total_size: 0,
        downloaded: 0,
        status: if start_paused {
            TaskStatus::Paused
        } else {
            TaskStatus::Pending
        },
        segments: Vec::new(),
        connections,
        speed_limit,
        error: None,
        created_at: now_ms(),
        finished_at: None,
        user_agent: req.user_agent.clone().unwrap_or_default(),
        headers: req.headers.clone().unwrap_or_default(),
        resumable: false,
    };

    let handle = TaskHandle::new(task.clone());
    {
        let mut map = engine.tasks.lock().await;
        map.insert(id.clone(), handle.clone());
    }

    // 「稍后开始」的任务不立即启动，等待用户手动继续
    if !start_paused {
        let e2 = engine.clone();
        let handle_c = handle.clone();
        let app_c = app.clone();
        let removed = handle.removed.clone();
        let worker = tokio::spawn(async move {
            tokio::select! {
                biased;
                _ = removed.notified() => {},
                _ = run_task(app_c, e2, handle_c) => {},
            }
        });
        *handle.worker.lock().await = Some(worker);
    }

    Ok(task)
}

/// 暂停
pub async fn pause_task(engine: &Arc<DownloadEngine>, id: &str) -> Result<(), String> {
    let map = engine.tasks.lock().await;
    let h = map.get(id).ok_or_else(|| "任务不存在".to_string())?;
    h.pause.store(true, Ordering::SeqCst);
    Ok(())
}

/// 恢复（重新启动运行循环，从分片进度续传）
pub async fn resume_task(
    app: tauri::AppHandle,
    engine: Arc<DownloadEngine>,
    id: &str,
) -> Result<(), String> {
    let handle = {
        let map = engine.tasks.lock().await;
        map.get(id)
            .cloned()
            .ok_or_else(|| "任务不存在".to_string())?
    };
    {
        let mut t = handle.task.lock().await;
        if t.status == TaskStatus::Completed
            || t.status.is_active()
            || t.status == TaskStatus::Pending
        {
            return Ok(());
        }
        t.status = TaskStatus::Pending;
        t.error = None;
    }
    handle.pause.store(false, Ordering::SeqCst);
    handle.cancel.store(false, Ordering::SeqCst);
    let e2 = engine.clone();
    let removed = handle.removed.clone();
    let handle_c = handle.clone();
    let worker = tokio::spawn(async move {
        tokio::select! {
            biased;
            _ = removed.notified() => {},
            _ = run_task(app, e2, handle_c) => {},
        }
    });
    *handle.worker.lock().await = Some(worker);
    Ok(())
}

/// 暂停全部进行中的任务
pub async fn pause_all(app: &tauri::AppHandle, engine: &Arc<DownloadEngine>) {
    let ids: Vec<String> = {
        let list = engine.snapshot().await;
        list.into_iter()
            .filter(|t| t.status.is_active())
            .map(|t| t.id)
            .collect()
    };
    for id in ids {
        let _ = pause_task(engine, &id).await;
    }
    let list = engine.snapshot().await;
    let _ = app.emit(EVT_TASKS, list);
}

/// 恢复全部暂停 / 失败的任务
pub async fn resume_all(app: &tauri::AppHandle, engine: &Arc<DownloadEngine>) {
    let ids: Vec<String> = {
        let list = engine.snapshot().await;
        list.into_iter()
            .filter(|t| t.status == TaskStatus::Paused || t.status == TaskStatus::Failed)
            .map(|t| t.id)
            .collect()
    };
    for id in ids {
        let _ = resume_task(app.clone(), Arc::clone(engine), &id).await;
    }
    let list = engine.snapshot().await;
    let _ = app.emit(EVT_TASKS, list);
}

/// 取消
pub async fn cancel_task(engine: &Arc<DownloadEngine>, id: &str) -> Result<(), String> {
    let map = engine.tasks.lock().await;
    let h = map.get(id).ok_or_else(|| "任务不存在".to_string())?;
    h.cancel.store(true, Ordering::SeqCst);
    h.pause.store(true, Ordering::SeqCst);
    Ok(())
}

/// 移除任务；保留本地数据，或将已下载文件及分片移到废纸篓。
pub async fn remove_task(
    engine: &Arc<DownloadEngine>,
    id: &str,
    delete_file: bool,
) -> Result<(), String> {
    let handle = {
        let map = engine.tasks.lock().await;
        map.get(id)
            .cloned()
            .ok_or_else(|| "任务不存在".to_string())?
    };
    handle.cancel.store(true, Ordering::SeqCst);
    handle.pause.store(true, Ordering::SeqCst);
    handle.removed.notify_one();
    if let Some(worker) = handle.worker.lock().await.take() {
        worker
            .await
            .map_err(|error| format!("停止下载失败：{error}"))?;
    }

    let task = {
        let mut task = handle.task.lock().await;
        if task.status != TaskStatus::Completed {
            task.status = TaskStatus::Cancelled;
            task.downloaded = task.segments.iter().map(|segment| segment.downloaded).sum();
            if !delete_file && !task.segments.is_empty() {
                save_state(&task);
            }
        }
        task.clone()
    };
    if delete_file {
        let mut paths = vec![PathBuf::from(&task.save_path), state_path(&task.save_path)];
        paths
            .extend((0..task.segments.len().max(1)).map(|index| part_path(&task.save_path, index)));
        let paths: Vec<_> = paths
            .into_iter()
            .filter(|path| fs::symlink_metadata(path).is_ok())
            .collect();
        if !paths.is_empty() {
            tokio::task::spawn_blocking(move || trash::delete_all(paths))
                .await
                .map_err(|error| error.to_string())?
                .map_err(|error| format!("无法将本地文件移到废纸篓：{error}"))?;
        }
    }
    engine.tasks.lock().await.remove(id);
    Ok(())
}

// ────────────────────────── 集成测试 ──────────────────────────
//
// 用一个内建的极简 HTTP 服务器（支持 HEAD / GET / Range）真实跑通：
//   1. 多线程分段下载 + 合并
//   2. 断点续传（模拟中断后从已下载位置继续）
//
// 注意：download_segment / merge_parts 不依赖 tauri::AppHandle，可独立测试。

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;

    #[test]
    fn old_settings_still_require_confirmation_and_keep_local_files() {
        let old: Settings = serde_json::from_str(r#"{"theme":"light","connections":12}"#).unwrap();
        assert!(old.confirm_remove_task);
        assert!(!old.remove_task_files);
        assert_eq!(old.connections, 12);
        assert_eq!(old.theme, crate::models::ThemeMode::Light);
        let remembered = Settings {
            confirm_remove_task: false,
            remove_task_files: true,
            ..old
        };
        let restored: Settings =
            serde_json::from_str(&serde_json::to_string(&remembered).unwrap()).unwrap();
        assert!(!restored.confirm_remove_task);
        assert!(restored.remove_task_files);
    }

    #[tokio::test]
    async fn removing_waits_for_writes_and_preserves_partial_data_when_requested() {
        let dir = std::env::temp_dir().join(format!("tdm_remove_keep_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let save = dir.join("keep.bin").to_string_lossy().into_owned();
        let engine = Arc::new(DownloadEngine::new());
        let mut task = make_task("keep", "https://example.com/keep", &save, 20, 1);
        task.status = TaskStatus::Downloading;
        let handle = TaskHandle::new(task);
        let removed = handle.removed.clone();
        let part = part_path(&save, 0);
        let worker_part = part.clone();
        *handle.worker.lock().await = Some(tokio::spawn(async move {
            removed.notified().await;
            tokio::time::sleep(Duration::from_millis(20)).await;
            fs::write(worker_part, b"last chunk").unwrap();
        }));
        engine.tasks.lock().await.insert("keep".into(), handle);
        remove_task(&engine, "keep", false).await.unwrap();
        assert_eq!(fs::read(&part).unwrap(), b"last chunk");
        assert!(state_path(&save).exists());
        assert!(engine.tasks.lock().await.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn removing_completed_task_can_trash_its_downloaded_file() {
        let dir = std::env::temp_dir().join(format!("tdm_remove_file_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let save = dir.join(format!("tdm-delete-test-{}.txt", uuid::Uuid::new_v4()));
        fs::write(&save, b"temporary test file").unwrap();
        let engine = Arc::new(DownloadEngine::new());
        let mut task = make_task(
            "completed",
            "https://example.com/file",
            &save.to_string_lossy(),
            19,
            1,
        );
        task.status = TaskStatus::Completed;
        engine
            .tasks
            .lock()
            .await
            .insert("completed".into(), TaskHandle::new(task));
        remove_task(&engine, "completed", true).await.unwrap();
        assert!(!save.exists());
        assert!(engine.tasks.lock().await.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    /// 启动一个支持 Range 的极简 HTTP 服务器，返回端口号
    fn start_test_server(data: Vec<u8>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定测试端口失败");
        let port = listener.local_addr().expect("获取端口失败").port();

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let data = data.clone();
                std::thread::spawn(move || {
                    let mut buf = [0u8; 8192];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let is_head = req.starts_with("HEAD");

                    // 解析 Range: bytes=start-end
                    let range = req.lines().find_map(|line| {
                        let l = line.to_lowercase();
                        if let Some(rest) = l.strip_prefix("range:") {
                            let v = rest.trim();
                            if let Some(v) = v.strip_prefix("bytes=") {
                                let parts: Vec<&str> = v.split('-').collect();
                                if parts.len() == 2 {
                                    let s: u64 = parts[0].trim().parse().unwrap_or(0);
                                    let e: u64 = parts[1]
                                        .trim()
                                        .parse()
                                        .unwrap_or_else(|_| data.len().saturating_sub(1) as u64);
                                    return Some((s, e.min(data.len().saturating_sub(1) as u64)));
                                }
                            }
                        }
                        None
                    });

                    let total = data.len() as u64;
                    let (status, start, end) = match range {
                        Some((s, e)) => ("206 Partial Content", s, e),
                        None => ("200 OK", 0u64, total.saturating_sub(1)),
                    };
                    let body = if is_head {
                        Vec::new()
                    } else {
                        data[start as usize..=(end as usize).min(data.len().saturating_sub(1))]
                            .to_vec()
                    };

                    let mut resp = format!(
                        "HTTP/1.1 {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n",
                        status,
                        if is_head { total } else { body.len() as u64 }
                    );
                    if let Some((s, e)) = range {
                        resp += &format!("Content-Range: bytes {}-{}/{}\r\n", s, e, total);
                    }
                    resp += "Content-Type: application/octet-stream\r\nConnection: close\r\n\r\n";

                    let _ = stream.write_all(resp.as_bytes());
                    if !is_head {
                        let _ = stream.write_all(&body);
                    }
                    let _ = stream.flush();
                });
            }
        });

        port
    }

    fn make_task(id: &str, url: &str, save: &str, total: u64, conns: usize) -> DownloadTask {
        DownloadTask {
            id: id.to_string(),
            url: url.to_string(),
            save_path: save.to_string(),
            filename: "out.bin".to_string(),
            total_size: total,
            downloaded: 0,
            status: TaskStatus::Pending,
            segments: build_segments(total, conns, &[]),
            connections: conns,
            speed_limit: 0,
            error: None,
            created_at: 0,
            finished_at: None,
            user_agent: String::new(),
            headers: Vec::new(),
            resumable: false,
        }
    }

    /// 用例 1：8 线程分段下载 → 合并，结果必须与源文件逐字节一致
    #[tokio::test]
    async fn test_segmented_download_and_merge() {
        let size = 700_000usize;
        let data: Vec<u8> = (0..size).map(|i| ((i * 7 + i / 13) % 251) as u8).collect();
        let port = start_test_server(data.clone());
        let url = format!("http://127.0.0.1:{}/test.bin", port);

        let dir = std::env::temp_dir().join(format!("tdm_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let save = dir.join("out.bin");
        let save_str = save.to_string_lossy().to_string();

        let conns = 8;
        let task = make_task("t1", &url, &save_str, size as u64, conns);
        let task_arc = Arc::new(Mutex::new(task));
        let client = reqwest::Client::new();
        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let limiter = Arc::new(Limiter::new(0));

        let segs = {
            let t = task_arc.lock().await;
            t.segments.clone()
        };
        assert_eq!(segs.len(), conns, "应切分为 {} 个分片", conns);

        for s in segs.iter() {
            download_segment(
                client.clone(),
                url.clone(),
                part_path(&save_str, s.index),
                s.index,
                s.start,
                s.end,
                false,
                task_arc.clone(),
                cancel.clone(),
                pause.clone(),
                limiter.clone(),
            )
            .await
            .expect("分片下载失败");
        }

        merge_parts(&save_str, conns).expect("合并失败");

        let got = fs::read(&save).expect("读取结果文件失败");
        assert_eq!(got.len(), size, "合并后长度不符");
        assert!(got == data, "合并后内容与源文件不一致！");

        // 分片文件应已被清理
        for i in 0..conns {
            assert!(!part_path(&save_str, i).exists(), "分片 {} 未清理", i);
        }

        let _ = fs::remove_dir_all(&dir);
    }

    /// 用例 2：断点续传——模拟中断后，从已下载位置继续，最终文件仍完整
    #[tokio::test]
    async fn test_resume_from_partial() {
        let size = 400_000usize;
        let data: Vec<u8> = (0..size).map(|i| ((i * 3 + 11) % 233) as u8).collect();
        let port = start_test_server(data.clone());
        let url = format!("http://127.0.0.1:{}/resume.bin", port);

        let dir = std::env::temp_dir().join(format!("tdm_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let save = dir.join("out.bin");
        let save_str = save.to_string_lossy().to_string();

        let conns = 4;
        let mut task = make_task("t2", &url, &save_str, size as u64, conns);

        // 模拟"上次已下载部分"：分片 0 下载了 40%，分片 1 下载了 10%，其余未开始
        let seg_total_0 = task.segments[0].total();
        let seg_total_1 = task.segments[1].total();
        task.segments[0].downloaded = seg_total_0 * 40 / 100;
        task.segments[1].downloaded = seg_total_1 * 10 / 100;
        fs::write(
            part_path(&save_str, 0),
            &data[0..task.segments[0].downloaded as usize],
        )
        .unwrap();
        fs::write(
            part_path(&save_str, 1),
            &data[task.segments[1].start as usize
                ..(task.segments[1].start + task.segments[1].downloaded) as usize],
        )
        .unwrap();

        let task_arc = Arc::new(Mutex::new(task));
        let client = reqwest::Client::new();
        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let limiter = Arc::new(Limiter::new(0));

        let segs = {
            let t = task_arc.lock().await;
            t.segments.clone()
        };

        // 续传：只下载未完成的部分
        for s in segs.iter() {
            download_segment(
                client.clone(),
                url.clone(),
                part_path(&save_str, s.index),
                s.index,
                s.start,
                s.end,
                false,
                task_arc.clone(),
                cancel.clone(),
                pause.clone(),
                limiter.clone(),
            )
            .await
            .expect("续传失败");
        }

        merge_parts(&save_str, conns).expect("合并失败");

        let got = fs::read(&save).expect("读取结果文件失败");
        assert_eq!(got.len(), size, "续传后长度不符");
        assert!(got == data, "续传后内容与源文件不一致！");

        let _ = fs::remove_dir_all(&dir);
    }

    /// 用例 3：纯函数校验——分片切分与文件名推断
    #[test]
    fn test_helpers() {
        // 分片切分：覆盖完整范围、不重叠、不遗漏
        let total = 1000u64;
        let segs = build_segments(total, 4, &[]);
        assert_eq!(segs.len(), 4);
        assert_eq!(segs[0].start, 0);
        assert_eq!(segs[3].end, total - 1);
        for i in 1..segs.len() {
            assert_eq!(segs[i].start, segs[i - 1].end + 1, "分片之间必须连续");
        }
        let sum: u64 = segs.iter().map(|s| s.total()).sum();
        assert_eq!(sum, total, "分片总长度必须等于文件大小");

        // 未知大小：单分片 unbounded
        let segs = build_segments(0, 8, &[]);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].end, u64::MAX);

        // 文件名推断
        assert_eq!(filename_from_url("https://a.com/file.zip"), "file.zip");
        assert_eq!(
            filename_from_url("https://a.com/path/to/video.mp4?token=1&x=2"),
            "video.mp4"
        );
        // 根路径无文件名时，回退到域名（非空的兜底名）
        let fallback = filename_from_url("https://a.com/");
        assert!(!fallback.is_empty(), "兜底文件名不应为空");
        assert_eq!(
            filename_from_url("https://a.com/%E6%B5%8B%E8%AF%95.zip"),
            "测试.zip"
        );

        // 非法字符清理
        assert_eq!(sanitize_filename("a/b:c*?.txt"), "abc.txt");
    }
}
