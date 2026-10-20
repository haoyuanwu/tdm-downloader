//! TDM Downloader 引擎 CLI 实测工具
//!
//! 直接调用项目下载引擎（probe → 分段 → 多线程下载 → 合并），
//! 用于在无 GUI 场景下验证真实大文件的多线程下载能力。
//!
//! 用法:
//!   cargo run --release --example download_cli -- <URL> <保存路径> [连接数]
//!
//! 示例:
//!   cargo run --release --example download_cli -- \
//!     https://huggingface.co/unsloth/Qwen3.8-27B-GGUF/resolve/main/Qwen3.8-27B-UD-IQ2_XXS.gguf \
//!     D:\models\Qwen3.8-27B-UD-IQ2_XXS.gguf 16

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use tdm_downloader_lib::engine::{
    build_http_client, build_segments, download_segment, merge_parts, part_path, probe, Limiter,
};
use tdm_downloader_lib::models::{DownloadTask, TaskStatus};

fn fmt_mb(bytes: u64) -> f64 {
    bytes as f64 / 1_048_576.0
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let url = args
        .get(1)
        .cloned()
        .ok_or_else(|| "用法: download_cli <URL> <保存路径> [连接数]".to_string())?;
    let save = args.get(2).cloned().ok_or_else(|| "缺少保存路径".to_string())?;
    let conns: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(16);

    println!("=== TDM Downloader 引擎 CLI 实测 ===");
    println!("URL : {}", url);
    println!("保存: {}", save);
    println!("连接: {}", conns);

    // 使用项目统一配置的客户端（自动识别 Windows 系统代理）
    let client = build_http_client();
    println!("      客户端已构建（自动代理检测）");

    // 1. 探测
    println!("[1/4] 探测资源 ...");
    let p = probe(&client, &url).await?;
    println!(
        "      大小 {} bytes ({:.0} MB)，支持分段下载: {}",
        p.total_size,
        fmt_mb(p.total_size),
        p.accept_ranges
    );
    if p.total_size == 0 {
        return Err("无法获取文件大小".to_string());
    }
    let conns = if p.accept_ranges { conns } else { 1 };
    if !p.accept_ranges {
        println!("      警告: 服务端不支持 Range，退化为单线程");
    }

    // 2. 切分（若已有 .partN 残留则按已有进度续传）
    let segments = build_segments(p.total_size, conns, &[]);
    println!(
        "[2/4] 切分 {} 个分片，每片约 {:.0} MB",
        segments.len(),
        fmt_mb(p.total_size / conns as u64)
    );

    let task = Arc::new(Mutex::new(DownloadTask {
        id: "cli".to_string(),
        url: url.clone(),
        save_path: save.clone(),
        filename: "cli.bin".to_string(),
        total_size: p.total_size,
        downloaded: 0,
        status: TaskStatus::Downloading,
        segments,
        connections: conns,
        speed_limit: 0,
        error: None,
        created_at: 0,
        finished_at: None,
    }));

    // 3. 进度打印协程
    let task_pr = task.clone();
    let printer = tokio::spawn(async move {
        let mut last: u64 = 0;
        let mut last_t = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let (d, total, done) = {
                let t = task_pr.lock().await;
                let d: u64 = t.segments.iter().map(|s| s.downloaded).sum();
                let done = t.segments.iter().all(|s| s.done);
                (d, t.total_size, done)
            };
            let now = Instant::now();
            let dt = now.duration_since(last_t).as_secs_f64().max(0.001);
            let speed = (d.saturating_sub(last)) as f64 / dt / 1_048_576.0;
            let pct = if total > 0 { d as f64 / total as f64 * 100.0 } else { 0.0 };
            println!(
                "      进度 {:5.1}%  {:7.0}/{:.0} MB   速度 {:5.1} MB/s",
                pct,
                fmt_mb(d),
                fmt_mb(total),
                speed
            );
            last = d;
            last_t = now;
            if done {
                break;
            }
        }
    });

    // 4. 多线程下载
    println!("[3/4] 开始多线程下载 ...");
    let start = Instant::now();
    let cancel = Arc::new(AtomicBool::new(false));
    let pause = Arc::new(AtomicBool::new(false));
    let limiter = Arc::new(Limiter::new(0));

    let segs = {
        let t = task.lock().await;
        t.segments.clone()
    };

    let mut handles = Vec::new();
    for s in segs.iter() {
        handles.push(tokio::spawn(download_segment(
            client.clone(),
            url.clone(),
            part_path(&save, s.index),
            s.index,
            s.start,
            s.end,
            false,
            task.clone(),
            cancel.clone(),
            pause.clone(),
            limiter.clone(),
        )));
    }

    let mut first_err: Option<String> = None;
    for h in handles {
        match h.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(format!("任务 panic: {}", e));
                }
            }
        }
    }
    let _ = printer.await;

    let elapsed = start.elapsed().as_secs_f64();
    if let Some(e) = first_err {
        println!("      下载中断（已保留 .part 分片，可重跑续传）: {}", e);
        return Err(e);
    }

    println!("      下载耗时 {:.1}s，开始合并 ...", elapsed);
    println!("[4/4] 合并分片 ...");
    merge_parts(&save, conns)?;

    let size = std::fs::metadata(&save).map(|m| m.len()).unwrap_or(0);
    println!("完成! 大小 {} bytes，耗时 {:.1}s，平均 {:.1} MB/s",
        size, elapsed, size as f64 / elapsed / 1_048_576.0);
    if size == p.total_size {
        println!("✅ SIZE_OK 与服务端声明大小一致");
    } else {
        println!("❌ SIZE_MISMATCH 期望 {} 实际 {}", p.total_size, size);
        return Err("大小校验失败".to_string());
    }
    Ok(())
}
