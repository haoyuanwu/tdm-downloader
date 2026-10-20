//! 多线程下载引擎
//!
//! 能力：
//! - 分段下载（HTTP Range，多连接并行）
//! - 断点续传（`.partN` 分片 + `.tdmstate.json` 状态持久化）
//! - 暂停 / 恢复 / 取消
//! - 限速（时间窗口配额）
//! - 分片合并

use crate::models::{
    DownloadTask, NewTaskRequest, ProgressEvent, SegmentProgress, Settings, TaskStatus,
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
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;

pub const DEFAULT_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) TDM-Downloader/0.1";

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

/// 构造统一配置的 HTTP 客户端（自动识别系统代理）
pub fn build_http_client() -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .user_agent(DEFAULT_UA)
        .connect_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10));

    let use_env = std::env::var("TDM_USE_ENV_PROXY")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    #[cfg(windows)]
    {
        if use_env {
            // 显式要求：交给 reqwest 默认逻辑（读环境变量）
        } else {
            match windows_system_proxy() {
                Some((addr, bypass)) => {
                    if let Some(url) = normalize_proxy_addr(&addr) {
                        if let Ok(p) = reqwest::Proxy::all(&url) {
                            let p = match bypass {
                                Some(b) if !b.trim().is_empty() => p.no_proxy(
                                    reqwest::NoProxy::from_string(
                                        &b.replace(';', ",").replace("<local>", ""),
                                    ),
                                ),
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

    #[cfg(not(windows))]
    {
        let _ = use_env; // 非 Windows 沿用 reqwest 默认（环境变量）
    }

    builder
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

// ────────────────────────── 限速器 ──────────────────────────

/// 基于 1 秒时间窗口的限速器。limit == 0 表示不限速。
pub struct Limiter {
    limit: AtomicU64,
    window: Mutex<(Instant, u64)>,
}

impl Limiter {
    pub fn new(limit: u64) -> Self {
        Limiter {
            limit: AtomicU64::new(limit),
            window: Mutex::new((Instant::now(), 0)),
        }
    }

    pub fn set_limit(&self, limit: u64) {
        self.limit.store(limit, Ordering::SeqCst);
    }

    /// 消费 n 字节配额，超出则休眠等待
    pub async fn take(&self, n: u64) {
        let limit = self.limit.load(Ordering::SeqCst);
        if limit == 0 || n == 0 {
            return;
        }
        let mut w = self.window.lock().await;
        let now = Instant::now();
        if now.duration_since(w.0) >= Duration::from_secs(1) {
            w.0 = now;
            w.1 = 0;
        }
        if w.1 + n > limit {
            let excess = w.1 + n - limit;
            let wait = Duration::from_secs_f64(excess as f64 / limit as f64);
            drop(w);
            tokio::time::sleep(wait).await;
            let mut w = self.window.lock().await;
            let now = Instant::now();
            if now.duration_since(w.0) >= Duration::from_secs(1) {
                w.0 = now;
                w.1 = 0;
            }
            w.1 += n;
        } else {
            w.1 += n;
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

pub fn persist_settings(s: &Settings) {
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let _ = fs::write(settings_file(), json);
    }
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
            &client,
            &url,
            &part,
            seg_index,
            start,
            end,
            unbounded,
            &task,
            &cancel,
            &pause,
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
    let status_ok =
        status == reqwest::StatusCode::PARTIAL_CONTENT || status.is_success();
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
pub fn build_segments(total_size: u64, connections: usize, prev: &[SegmentProgress]) -> Vec<SegmentProgress> {
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
            out.write_all(&data).map_err(|e| format!("写入合并失败: {}", e))?;
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
pub async fn run_task(
    app: tauri::AppHandle,
    engine: Arc<DownloadEngine>,
    handle: TaskHandle,
) {
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
        let id_c = id.clone();
        tokio::spawn(async move {
            let mut last: u64 = 0;
            let mut last_time = Instant::now();
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                if cancel.load(Ordering::SeqCst) {
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

    let probe_res = probe(&engine.client, &url).await;
    let (total_size, accept_ranges, server_filename) = match probe_res {
        Ok(p) => (p.total_size, p.accept_ranges, p.filename),
        Err(e) => {
            let mut t = handle.task.lock().await;
            t.status = TaskStatus::Failed;
            t.error = Some(format!("无法连接资源: {}", e));
            mon_cancel.store(true, Ordering::SeqCst);
            let _ = mon.await;
            notify(&app, &engine, t.clone()).await;
            return;
        }
    };

    // 文件名：请求指定 > 服务端 Content-Disposition > URL 推断
    {
        let mut t = handle.task.lock().await;
        if t.filename.is_empty() || t.filename == "download" {
            if let Some(f) = server_filename.clone() {
                t.filename = f;
            }
        }
        let dir = Path::new(&t.save_path).parent().unwrap_or(Path::new(".")).to_path_buf();
        let _ = fs::create_dir_all(&dir);
        let full = unique_path(&dir, &t.filename);
        t.filename = full
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| t.filename.clone());
        t.save_path = full.to_string_lossy().to_string();
        t.total_size = total_size;
    }

    let (save_path, total_size, unbounded) = {
        let t = handle.task.lock().await;
        (t.save_path.clone(), t.total_size, t.total_size == 0)
    };

    // 2. 构建/恢复分片
    {
        let settings = engine.settings.lock().await;
        let mut t = handle.task.lock().await;
        let prev = load_state(&save_path).map(|s| s.segments).unwrap_or_default();
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

    let mut futs = Vec::new();
    for s in segs_snapshot.iter() {
        if s.done {
            continue;
        }
        let client = engine.client.clone();
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
        futs.push(tokio::spawn(async move {
            download_segment(
                client, url_c, part, idx, start, end, unbounded_c, task_c,
                cancel_c, pause_c, limiter_c,
            )
            .await
        }));
    }

    let mut first_error: Option<String> = None;
    for f in futs {
        match f.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(format!("任务异常: {}", e));
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
}

/// 推送单个任务更新 + 任务列表更新
async fn notify(app: &tauri::AppHandle, engine: &Arc<DownloadEngine>, task: DownloadTask) {
    let _ = app.emit(format!("download://task/{}", task.id).as_str(), task);
    let list = engine.snapshot().await;
    let _ = app.emit(EVT_TASKS, list);
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
    let task = DownloadTask {
        id: id.clone(),
        url: req.url.clone(),
        save_path: dir_path.join(&filename).to_string_lossy().to_string(),
        filename,
        total_size: 0,
        downloaded: 0,
        status: TaskStatus::Pending,
        segments: Vec::new(),
        connections,
        speed_limit,
        error: None,
        created_at: now_ms(),
        finished_at: None,
    };

    let handle = TaskHandle::new(task.clone());
    {
        let mut map = engine.tasks.lock().await;
        map.insert(id.clone(), handle.clone());
    }

    let e2 = engine.clone();
    tokio::spawn(async move {
        run_task(app, e2, handle).await;
    });

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
        map.get(id).cloned().ok_or_else(|| "任务不存在".to_string())?
    };
    {
        let mut t = handle.task.lock().await;
        if t.status == TaskStatus::Completed || t.status == TaskStatus::Downloading {
            return Ok(());
        }
        t.status = TaskStatus::Pending;
        t.error = None;
    }
    handle.pause.store(false, Ordering::SeqCst);
    handle.cancel.store(false, Ordering::SeqCst);
    let e2 = engine.clone();
    tokio::spawn(async move {
        run_task(app, e2, handle).await;
    });
    Ok(())
}

/// 取消
pub async fn cancel_task(engine: &Arc<DownloadEngine>, id: &str) -> Result<(), String> {
    let map = engine.tasks.lock().await;
    let h = map.get(id).ok_or_else(|| "任务不存在".to_string())?;
    h.cancel.store(true, Ordering::SeqCst);
    h.pause.store(true, Ordering::SeqCst);
    Ok(())
}

/// 删除任务（并清理分片/状态文件，可选同时删除已下载文件）
pub async fn remove_task(engine: &Arc<DownloadEngine>, id: &str, delete_file: bool) -> Result<(), String> {
    let handle = {
        let mut map = engine.tasks.lock().await;
        map.remove(id).ok_or_else(|| "任务不存在".to_string())?
    };
    handle.cancel.store(true, Ordering::SeqCst);
    handle.pause.store(true, Ordering::SeqCst);

    let t = handle.task.lock().await;
    let n = t.segments.len().max(1);
    for i in 0..n {
        let _ = fs::remove_file(part_path(&t.save_path, i));
    }
    let _ = fs::remove_file(state_path(&t.save_path));
    if delete_file && !matches!(t.status, TaskStatus::Completed) {
        let _ = fs::remove_file(PathBuf::from(&t.save_path));
    }
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
                        data[start as usize..=(end as usize).min(data.len().saturating_sub(1))].to_vec()
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
        fs::write(part_path(&save_str, 0), &data[0..task.segments[0].downloaded as usize]).unwrap();
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
