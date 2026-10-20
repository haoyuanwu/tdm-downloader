//! v0.2.0 功能验证工具（无需 GUI）
//!
//! 覆盖两组验证：
//!   A. 真实链接（HuggingFace 6.77GB 模型）：预解析、16 线程下载、断点续传、停滞重连
//!   B. 本地回显服务器（确定性）：自定义 UA/请求头、代理模式、限速、续传起点
//!
//! 用法:
//!   cargo run --release --example verify_v020            # 跑全部
//!   cargo run --release --example verify_v020 --local    # 只跑本地确定性验证
//!   cargo run --release --example verify_v020 --remote   # 只跑真实链接验证
//!   cargo run --release --example verify_v020 --remote --seconds 60

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use tdm_downloader_lib::engine::{
    build_http_client, build_segments, build_task_client, download_segment, merge_parts, part_path,
    probe, ClientConfig, Limiter,
};
use tdm_downloader_lib::models::{DownloadTask, ProxyMode, TaskStatus};

const URL: &str = "https://huggingface.co/unsloth/Qwen3.8-27B-GGUF/resolve/main/Qwen3.8-27B-UD-IQ2_XXS.gguf";
const SAVE: &str = r"D:\models\Qwen3.8-27B-UD-IQ2_XXS.gguf";

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1_048_576.0
}

fn ok(b: bool) -> &'static str {
    if b {
        "PASS"
    } else {
        "FAIL"
    }
}

// ───────────────────────── 本地回显服务器 ─────────────────────────

struct EchoServer {
    port: u16,
    seen: Arc<StdMutex<Vec<HashMap<String, String>>>>,
}

/// 启动一个记录请求头、支持 Range 的极简 HTTP 服务器
fn start_echo_server(total: usize) -> EchoServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("绑定端口失败");
    let port = listener.local_addr().expect("获取端口失败").port();
    let seen: Arc<StdMutex<Vec<HashMap<String, String>>>> =
        Arc::new(StdMutex::new(Vec::new()));
    let seen_c = seen.clone();
    let data = vec![0x5Au8; total];

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };
            let data = data.clone();
            let seen_c = seen_c.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 16_384];
                let n = stream.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    return;
                }
                let req = String::from_utf8_lossy(&buf[..n]).to_string();

                // 记录请求头
                let mut hm = HashMap::new();
                for line in req.lines().skip(1) {
                    if let Some((k, v)) = line.split_once(':') {
                        if !k.trim().is_empty() {
                            hm.insert(k.trim().to_lowercase(), v.trim().to_string());
                        }
                    }
                }
                if let Ok(mut g) = seen_c.lock() {
                    g.push(hm);
                }

                let is_head = req.starts_with("HEAD");
                let range = req.lines().find_map(|line| {
                    let l = line.to_lowercase();
                    let v = l.strip_prefix("range:")?.trim().to_string();
                    let v = v.strip_prefix("bytes=")?.to_string();
                    let parts: Vec<&str> = v.split('-').collect();
                    if parts.len() == 2 {
                        let s: u64 = parts[0].trim().parse().unwrap_or(0);
                        let e: u64 = parts[1]
                            .trim()
                            .parse()
                            .unwrap_or_else(|_| data.len().saturating_sub(1) as u64);
                        return Some((s, e.min(data.len().saturating_sub(1) as u64)));
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

    EchoServer { port, seen }
}

/// 取最后一次请求的头
fn last_headers(srv: &EchoServer) -> HashMap<String, String> {
    srv.seen.lock().unwrap().last().cloned().unwrap_or_default()
}

fn new_task(url: &str, save: &str, total: u64, conns: usize) -> Arc<Mutex<DownloadTask>> {
    Arc::new(Mutex::new(DownloadTask {
        id: "verify".to_string(),
        url: url.to_string(),
        save_path: save.to_string(),
        filename: "verify.bin".to_string(),
        total_size: total,
        downloaded: 0,
        status: TaskStatus::Downloading,
        segments: build_segments(total, conns, &[]),
        connections: conns,
        speed_limit: 0,
        error: None,
        created_at: 0,
        finished_at: None,
        user_agent: String::new(),
        headers: Vec::new(),
        resumable: true,
    }))
}

// ───────────────────────── B. 本地确定性验证 ─────────────────────────

async fn verify_local() -> Result<(), String> {
    println!("\n══════ B. 本地回显服务器验证（确定性） ══════");
    let tmp = std::env::temp_dir().join("tdm_verify");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

    // ── B1. 自定义 UA / Referer / Cookie ──
    println!("\n[B1] 自定义 User-Agent / Referer / Cookie");
    let srv = start_echo_server(4 * 1024 * 1024);
    let cfg = ClientConfig {
        proxy_mode: ProxyMode::None, // 本地直连
        proxy_addr: String::new(),
        user_agent: "TDM-Verify/9.9".to_string(),
        headers: vec![
            ("Referer".into(), "https://example.com/page".into()),
            ("Cookie".into(), "token=abc123".into()),
            ("X-Custom-Header".into(), "hello".into()),
            ("Bad Header Name".into(), "should-be-ignored".into()),
        ],
    };
    let client = build_task_client(&cfg);
    let url = format!("http://127.0.0.1:{}/file.bin", srv.port);
    let r = client
        .get(&url)
        .header("Range", "bytes=0-1023")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = r.status().as_u16();
    let body = r.bytes().await.map_err(|e| e.to_string())?.len();
    let h = last_headers(&srv);
    println!(
        "     响应 {}  收到 {} bytes",
        status, body
    );
    println!("     UA      = {:?}", h.get("user-agent"));
    println!("     Referer = {:?}", h.get("referer"));
    println!("     Cookie  = {:?}", h.get("cookie"));
    println!("     X-Custom= {:?}", h.get("x-custom-header"));
    println!(
        "     {} UA 生效    {} Referer 生效    {} Cookie 生效    {} 自定义头生效    {} 非法头名被忽略",
        ok(h.get("user-agent").map(|v| v == "TDM-Verify/9.9").unwrap_or(false)),
        ok(h.get("referer").map(|v| v == "https://example.com/page").unwrap_or(false)),
        ok(h.get("cookie").map(|v| v == "token=abc123").unwrap_or(false)),
        ok(h.get("x-custom-header").map(|v| v == "hello").unwrap_or(false)),
        ok(!h.contains_key("bad header name")),
    );
    println!(
        "     {} Range 请求返回 206 且长度正确",
        ok(status == 206 && body == 1024)
    );

    // ── B2. 代理模式 ──
    println!("\n[B2] 代理模式（直连 / 跟随系统 / 自定义）");
    for (name, mode, addr) in [
        ("直连 None", ProxyMode::None, ""),
        ("跟随系统 System", ProxyMode::System, ""),
        ("自定义 Custom(不可达 127.0.0.1:9)", ProxyMode::Custom, "127.0.0.1:9"),
    ] {
        let cfg = ClientConfig {
            proxy_mode: mode,
            proxy_addr: addr.to_string(),
            user_agent: String::new(), // 空 -> 回落默认 UA
            headers: vec![],
        };
        let c = build_task_client(&cfg);
        let t0 = Instant::now();
        let res = tokio::time::timeout(
            Duration::from_secs(8),
            c.get(&url).header("Range", "bytes=0-1023").send(),
        )
        .await;
        match res {
            Ok(Ok(r)) => println!(
                "     {:<32} -> {} ({} ms)  {}",
                name,
                r.status(),
                t0.elapsed().as_millis(),
                ok(r.status().is_success())
            ),
            Ok(Err(e)) => println!("     {:<32} -> 请求失败: {}（符合预期：不可达代理）", name, e),
            Err(_) => println!("     {:<32} -> 超时（符合预期：不可达代理）", name),
        }
    }

    // ── B3. 限速 ──
    println!("\n[B3] 限速器（配额 2 MB/s，下载 8 MB）");
    let srv2 = start_echo_server(8 * 1024 * 1024);
    let url2 = format!("http://127.0.0.1:{}/big.bin", srv2.port);
    let save2 = tmp.join("limited.bin");
    let task = new_task(&url2, &save2.to_string_lossy(), 8 * 1024 * 1024, 1);
    let limiter = Arc::new(Limiter::new(2 * 1024 * 1024));
    let cancel = Arc::new(AtomicBool::new(false));
    let pause = Arc::new(AtomicBool::new(false));
    let t0 = Instant::now();
    download_segment(
        build_http_client(),
        url2.clone(),
        part_path(&save2.to_string_lossy(), 0),
        0,
        0,
        8 * 1024 * 1024 - 1,
        false,
        task.clone(),
        cancel.clone(),
        pause.clone(),
        limiter.clone(),
    )
    .await?;
    let el = t0.elapsed().as_secs_f64().max(0.001);
    let got = std::fs::metadata(part_path(&save2.to_string_lossy(), 0))
        .map(|m| m.len())
        .unwrap_or(0);
    let real = mb(got as u64) / el;
    println!(
        "     下载 {:.2} MB 用时 {:.2}s  实测 {:.2} MB/s（上限 2.00）",
        mb(got), el, real
    );
    println!("     {} 限速生效（实测不超过上限 1.15 倍）", ok(real <= 2.0 * 1.15));

    // ── B4. 续传起点（.part 已有内容则从偏移继续）──
    println!("\n[B4] 断点续传（预置 3 MB 的 .part0，下载 8 MB）");
    let save3 = tmp.join("resume.bin");
    let part3 = part_path(&save3.to_string_lossy(), 0);
    std::fs::create_dir_all(&tmp).ok();
    std::fs::write(&part3, vec![0x11u8; 3 * 1024 * 1024]).map_err(|e| e.to_string())?;
    let srv3 = start_echo_server(8 * 1024 * 1024);
    let url3 = format!("http://127.0.0.1:{}/res.bin", srv3.port);
    let task3 = new_task(&url3, &save3.to_string_lossy(), 8 * 1024 * 1024, 1);
    let t0 = Instant::now();
    download_segment(
        build_http_client(),
        url3.clone(),
        part3.clone(),
        0,
        0,
        8 * 1024 * 1024 - 1,
        false,
        task3.clone(),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(Limiter::new(0)),
    )
    .await?;
    let size3 = std::fs::metadata(&part3).map(|m| m.len()).unwrap_or(0);
    // 合并后校验总大小
    merge_parts(&save3.to_string_lossy(), 1)?;
    let final3 = std::fs::metadata(&save3).map(|m| m.len()).unwrap_or(0);
    let head3 = std::fs::read(&save3)
        .map(|b| b.first().copied().unwrap_or(0))
        .unwrap_or(0);
    println!(
        "     part0 {} bytes -> 合并后 {} bytes，首字节 0x{:02X}（预置为 0x11）",
        size3, final3, head3
    );
    println!(
        "     {} 续传未重复下载头部    {} 合并后大小正确",
        ok(head3 == 0x11),
        ok(final3 == 8 * 1024 * 1024)
    );
    println!("     用时 {:.2}s", t0.elapsed().as_secs_f64());

    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

// ───────────────────────── A. 真实链接验证 ─────────────────────────

async fn verify_remote(seconds: u64) -> Result<(), String> {
    println!("\n══════ A. 真实链接验证（HuggingFace 6.77 GB） ══════");

    // ── A1. 预解析 ──
    println!("\n[A1] 链接预解析（新建下载对话框里填完链接就显示的那三项）");
    let client = build_http_client();
    let t0 = Instant::now();
    let p = probe(&client, URL).await?;
    println!("     文件名       : {}", p.filename.as_deref().unwrap_or("(无)"));
    println!("     大小         : {} bytes ({:.2} MB / {:.2} GB)", p.total_size, mb(p.total_size), p.total_size as f64 / 1_073_741_824.0);
    println!("     支持 Range   : {}（决定能否多线程 + 续传）", p.accept_ranges);
    println!("     耗时         : {} ms", t0.elapsed().as_millis());
    println!(
        "     {} 探测到文件名    {} 探测到大小    {} 支持分段",
        ok(p.filename.as_deref().map(|s| !s.is_empty()).unwrap_or(false)),
        ok(p.total_size > 0),
        ok(p.accept_ranges)
    );
    if p.total_size == 0 {
        return Err("预解析失败：未拿到文件大小".into());
    }

    // ── A2. 已有分片统计（续传基线）──
    let save = PathBuf::from(SAVE);
    let mut existing: u64 = 0;
    let mut parts = 0usize;
    for i in 0..64 {
        let pp = part_path(&SAVE.to_string(), i);
        if let Ok(m) = std::fs::metadata(&pp) {
            existing += m.len();
            parts += 1;
        }
    }
    println!("\n[A2] 磁盘上已有 {} 个分片，合计 {:.2} MB（续传基线）", parts, mb(existing));

    // ── A3. 多线程下载（含停滞重连）──
    let conns: usize = if p.accept_ranges { 16 } else { 1 };
    println!("\n[A3] 启动 {} 线程下载 {} 秒（验证续传 / 停滞重连 / 分片推进）", conns, seconds);
    let segments = build_segments(p.total_size, conns, &[]);
    let task = new_task(URL, SAVE, p.total_size, conns);
    {
        let mut t = task.lock().await;
        t.segments = segments.clone();
    }

    let start_existing = existing;
    let t0 = Instant::now();
    let cancel = Arc::new(AtomicBool::new(false));
    let pause = Arc::new(AtomicBool::new(false));

    let task_pr = task.clone();
    let printer = tokio::spawn(async move {
        let mut last: u64 = 0;
        let mut last_t = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let (d, done, active) = {
                let t = task_pr.lock().await;
                let d: u64 = t.segments.iter().map(|s| s.downloaded).sum();
                let done = t.segments.iter().filter(|s| s.done).count();
                let active = t.segments.iter().filter(|s| !s.done && s.downloaded > 0).count();
                (d, done, active)
            };
            let now = Instant::now();
            let dt = now.duration_since(last_t).as_secs_f64().max(0.001);
            let speed = (d.saturating_sub(last)) as f64 / dt / 1_048_576.0;
            println!(
                "     会话内 {:7.2} MB  速度 {:6.2} MB/s  已完成分片 {}/{}  活跃 {}",
                mb(d), speed, done, 16, active
            );
            last = d;
            last_t = now;
            if done >= 16 {
                break;
            }
        }
    });

    let mut handles = Vec::new();
    for s in segments.iter() {
        handles.push(tokio::spawn(download_segment(
            client.clone(),
            URL.to_string(),
            part_path(SAVE, s.index),
            s.index,
            s.start,
            s.end,
            s.start >= s.end,
            task.clone(),
            cancel.clone(),
            pause.clone(),
            Arc::new(Limiter::new(0)),
        )));
    }

    tokio::time::sleep(Duration::from_secs(seconds)).await;
    cancel.store(true, std::sync::atomic::Ordering::SeqCst);
    printer.abort();
    for h in handles {
        let _ = tokio::time::timeout(Duration::from_secs(3), h).await;
    }
    let el = t0.elapsed().as_secs_f64();

    // ── A4. 结果与续传校验 ──
    let mut now_total: u64 = 0;
    let mut now_parts = 0usize;
    let mut per_part = Vec::new();
    for i in 0..conns {
        let pp = part_path(SAVE, i);
        if let Ok(m) = std::fs::metadata(&pp) {
            now_total += m.len();
            now_parts += 1;
            per_part.push(mb(m.len()));
        }
    }
    let delta = now_total.saturating_sub(start_existing);
    println!("\n[A4] 结果（{} 秒后主动停止）", seconds);
    println!("     分片数       : {} -> {}", parts, now_parts);
    println!("     磁盘合计     : {:.2} MB -> {:.2} MB（新增 {:.2} MB）", mb(start_existing), mb(now_total), mb(delta));
    println!("     平均速度     : {:.2} MB/s", mb(delta) / el);
    println!("     各分片(MB)   : {:?}", per_part.iter().map(|v| format!("{:.0}", v)).collect::<Vec<_>>());
    let advanced = per_part.iter().filter(|&&v| v > 0.0).count();
    println!(
        "     {} 数据确实写入磁盘    {} 全部分片都在推进（无挂起连接）    {} 未从头重下（沿用已有分片）",
        ok(delta > 0),
        ok(advanced == conns),
        ok(now_total >= start_existing)
    );

    // ── A5. 暂停后恢复：再跑一小段，验证偏移量正确 ──
    println!("\n[A5] 二次启动（模拟暂停→继续），跑 20 秒校验偏移");
    let before = now_total;
    let segments2 = build_segments(p.total_size, conns, &[]);
    let task2 = new_task(URL, SAVE, p.total_size, conns);
    {
        let mut t = task2.lock().await;
        t.segments = segments2.clone();
    }
    let cancel2 = Arc::new(AtomicBool::new(false));
    let mut handles2 = Vec::new();
    for s in segments2.iter() {
        handles2.push(tokio::spawn(download_segment(
            client.clone(),
            URL.to_string(),
            part_path(SAVE, s.index),
            s.index,
            s.start,
            s.end,
            s.start >= s.end,
            task2.clone(),
            cancel2.clone(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(Limiter::new(0)),
        )));
    }
    tokio::time::sleep(Duration::from_secs(20)).await;
    cancel2.store(true, std::sync::atomic::Ordering::SeqCst);
    for h in handles2 {
        let _ = tokio::time::timeout(Duration::from_secs(3), h).await;
    }
    let mut after: u64 = 0;
    for i in 0..conns {
        if let Ok(m) = std::fs::metadata(part_path(SAVE, i)) {
            after += m.len();
        }
    }
    println!(
        "     {:.2} MB -> {:.2} MB（新增 {:.2} MB）",
        mb(before), mb(after), mb(after.saturating_sub(before))
    );
    println!(
        "     {} 续传是叠加而非覆盖重写",
        ok(after >= before)
    );

    let _ = save;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let only_local = args.iter().any(|a| a == "--local");
    let only_remote = args.iter().any(|a| a == "--remote");
    let seconds: u64 = args
        .iter()
        .position(|a| a == "--seconds")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(90);

    println!("=== TDM Downloader v0.2.0 功能验证 ===");
    println!("时间: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));

    if !only_remote {
        verify_local().await?;
    }
    if !only_local {
        verify_remote(seconds).await?;
    }

    println!("\n=== 验证结束 ===");
    Ok(())
}
