//! Chrome/Edge Native Messaging host. stdout is reserved for framed JSON replies.
#[path = "../../../src-tauri/src/browser_paths.rs"]
mod browser_paths;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

fn read_message(reader: &mut impl Read) -> Result<serde_json::Value, String> {
    let mut prefix = [0; 4];
    reader.read_exact(&mut prefix).map_err(|_| "缺少消息长度")?;
    let size = u32::from_le_bytes(prefix) as usize;
    if size == 0 || size > 32768 {
        return Err("消息长度无效".into());
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes).map_err(|_| "消息不完整")?;
    serde_json::from_slice(&bytes).map_err(|_| "消息不是有效 JSON".into())
}

fn target(message: &serde_json::Value) -> Result<url::Url, String> {
    let value = message["url"].as_str().ok_or("缺少下载链接")?;
    let url = url::Url::parse(value).map_err(|_| "链接无效")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("仅支持没有内嵌密码的 HTTP(S) 链接".into());
    }
    Ok(url)
}

fn reply(writer: &mut impl Write, value: &serde_json::Value) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

fn run(message: serde_json::Value) -> Result<serde_json::Value, String> {
    let target = target(&message)?;
    let directory = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("连接器目录无效")?
        .to_path_buf();
    let app_path = std::fs::read_to_string(directory.join("app-path.txt"))
        .map_err(|_| "找不到配置的 TDM，请重新配置浏览器连接器")?;
    let app_path = std::path::Path::new(app_path.trim());
    if !app_path.is_absolute() || !app_path.is_file() {
        return Err("配置的 TDM 程序不存在，请重新配置连接器".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let decisions = browser_paths::decision_dir()?;
    std::fs::create_dir_all(&decisions).map_err(|e| e.to_string())?;
    let decision = decisions.join(&id);
    let mut request = url::Url::parse("tdm://download").unwrap();
    request
        .query_pairs_mut()
        .append_pair("url", target.as_str())
        .append_pair("request", &id)
        .append_pair("filename", message["filename"].as_str().unwrap_or(""));
    if request.as_str().len() > 32768 {
        return Err("下载请求过长".into());
    }
    let mut command = std::process::Command::new(app_path);
    command
        .arg(request.as_str())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|e| format!("无法启动 TDM：{e}"))?;
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        if let Ok(result) = std::fs::read_to_string(&decision) {
            let _ = std::fs::remove_file(&decision);
            return match result.as_str() {
                "accepted" | "cancelled" => Ok(serde_json::json!({"status": result})),
                _ => Err("下载确认结果无效".into()),
            };
        }
        // Reap a second-instance launcher; the primary instance continues handling the request.
        let _ = child.try_wait();
        std::thread::sleep(Duration::from_millis(250));
    }
    let _ = std::fs::remove_file(decision);
    Ok(serde_json::json!({"status": "timeout", "error": "确认超时，浏览器继续下载"}))
}

fn main() {
    let response = read_message(&mut std::io::stdin())
        .and_then(run)
        .unwrap_or_else(|error| serde_json::json!({"status": "error", "error": error}));
    let _ = reply(&mut std::io::stdout(), &response);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_handles_partial_reads_and_rejects_truncated_or_oversized_input() {
        let value = serde_json::json!({"url":"https://example.com/文件.zip?a=1&b=2"});
        let mut frame = Vec::new();
        reply(&mut frame, &value).unwrap();
        assert_eq!(read_message(&mut frame.as_slice()).unwrap(), value);
        assert!(read_message(&mut frame[..frame.len() - 1].as_ref()).is_err());
        assert!(read_message(&mut 32769u32.to_le_bytes().as_ref()).is_err());
        assert!(read_message(&mut 0u32.to_le_bytes().as_ref()).is_err());
    }
    #[test]
    fn browser_cannot_launch_local_files_or_embed_credentials() {
        for value in [
            "file:///C:/Windows/notepad.exe",
            "javascript:alert(1)",
            "https://user:pass@example.com/a",
            "data:text/plain,hello",
        ] {
            assert!(target(&serde_json::json!({"url":value})).is_err());
        }
        assert!(target(&serde_json::json!({"url":"https://example.com/a.zip"})).is_ok());
    }
}
