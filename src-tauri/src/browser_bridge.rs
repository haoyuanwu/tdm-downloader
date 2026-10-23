use tauri::Manager;
use url::Url;
use uuid::Uuid;

fn parse(url: &Url) -> Result<super::QuickOpenPayload, String> {
    let supported_scheme = url.scheme() == "tdm"
        || (cfg!(debug_assertions) && url.scheme() == "tdm-dev");
    if url.as_str().len() > 32768 || !supported_scheme || url.host_str() != Some("download") {
        return Err("无效的浏览器下载请求".into());
    }
    let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let target = Url::parse(params.get("url").ok_or("缺少下载链接")?).map_err(|_| "链接无效")?;
    if !matches!(target.scheme(), "http" | "https") || target.host_str().is_none()
        || !target.username().is_empty() || target.password().is_some() {
        return Err("仅支持没有内嵌密码的 HTTP(S) 链接".into());
    }
    let request_id = params.get("request").map(|id| Uuid::parse_str(id).map(|id| id.to_string()))
        .transpose().map_err(|_| "请求标识无效")?;
    let filename = params.get("filename").filter(|name| !name.is_empty()).map(|name| {
        name.rsplit(['/', '\\']).next().unwrap_or("下载文件").chars().filter(|c| !c.is_control()).take(240).collect()
    });
    Ok(super::QuickOpenPayload { mode: "browser".into(), url: Some(target.into()), filename, request_id })
}

pub fn receive(app: &tauri::AppHandle, url: &Url) {
    match parse(url) {
        Ok(payload) => {
            // Set this before scheduling AppKit work: Reopen may arrive first.
            app.state::<super::BrowserWakeState>().0.store(true, std::sync::atomic::Ordering::Release);
            super::browser_dialog::present(app, payload);
        }
        Err(error) => eprintln!("浏览器接管失败：{error}"),
    }
}

#[tauri::command]
pub fn browser_download_decision(app: tauri::AppHandle, request_id: String, accepted: bool) -> Result<(), String> {
    let id = Uuid::parse_str(&request_id).map_err(|_| "请求标识无效")?.to_string();
    let current = app.state::<super::QuickWindowState>();
    let mut state = current.0.lock().map_err(|e| e.to_string())?;
    if state.request_id.as_deref() != Some(&id) { return Err("下载请求已过期，请在浏览器重试".into()); }
    let directory = super::browser_paths::decision_dir()?;
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    std::fs::write(directory.join(id), if accepted { "accepted" } else { "cancelled" }).map_err(|e| e.to_string())?;
    state.request_id = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_external_download_requests() {
        let request = Url::parse("tdm://download?url=https%3A%2F%2Fexample.com%2Fa%3Fx%3D1%26y%3D2&filename=..%2Ffile.zip").unwrap();
        let parsed = parse(&request).unwrap();
        assert_eq!(parsed.filename.as_deref(), Some("file.zip"));
        assert_eq!(parsed.url.as_deref(), Some("https://example.com/a?x=1&y=2"));
        for value in ["tdm://download?url=file:///etc/passwd", "tdm://download?url=https://user:pass@example.com", "tdm://download?url=https://example.com&request=../escape", "https://download?url=https://example.com"] {
            assert!(parse(&Url::parse(value).unwrap()).is_err());
        }
    }
}
