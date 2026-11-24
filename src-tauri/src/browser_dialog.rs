use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
use std::time::Duration;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use tauri::{Emitter, Manager};
use url::Url;

use super::engine::{build_task_client, filename_from_cd, ClientConfig, DownloadEngine};

#[derive(Clone, serde::Serialize)]
pub struct BrowserDialogContext {
    id: u64,
    payload: super::QuickOpenPayload,
}

#[derive(Default)]
pub struct BrowserDialogState {
    sequence: AtomicU64,
    pending: Mutex<Option<BrowserDialogContext>>,
}

#[tauri::command]
pub fn browser_dialog_context(app: tauri::AppHandle) -> Option<BrowserDialogContext> {
    app.state::<BrowserDialogState>().pending.lock().ok()?.clone()
}

#[tauri::command]
pub async fn browser_dialog_filename(
    app: tauri::AppHandle,
    id: u64,
) -> Result<String, String> {
    let payload = browser_dialog_context(app.clone())
        .filter(|context| context.id == id)
        .ok_or("下载请求已更新，请确认新的下载链接")?
        .payload;
    let url = Url::parse(payload.url.as_deref().ok_or("缺少下载链接")?)
        .map_err(|_| "链接无效")?;
    let browser_name = payload.filename.as_deref();
    if let Some(name) = browser_name.filter(|name| extension(name).is_some()) {
        return Ok(limit_filename(name));
    }
    let engine = app.state::<std::sync::Arc<DownloadEngine>>();
    let config = {
        let settings = engine.settings.lock().await;
        ClientConfig {
            proxy_mode: settings.proxy_mode,
            proxy_addr: settings.proxy_addr.clone(),
            ..Default::default()
        }
    };
    let response = build_task_client(&config)
        .head(url.clone())
        .timeout(Duration::from_secs(3))
        .send()
        .await
        .ok()
        .filter(|response| response.status().is_success());
    let server_name = response.as_ref().and_then(|response| {
        filename_from_cd(response.headers().get(CONTENT_DISPOSITION)?.to_str().ok()?)
            .and_then(|name| clean_filename(&name))
    });
    let mime = response.as_ref().and_then(|response| response.headers().get(CONTENT_TYPE)?.to_str().ok());
    let final_url = response.as_ref().map(reqwest::Response::url).unwrap_or(&url);
    Ok(resolve_filename(browser_name, server_name.as_deref(), final_url, mime))
}

pub(crate) fn clean_filename(name: &str) -> Option<String> {
    let name = name.rsplit(['/', '\\']).next()?;
    let name: String = name.chars().filter(|c| !c.is_control() && !"<>:\"|?*".contains(*c)).collect();
    let name = name.trim().trim_end_matches('.').trim_end();
    (!name.is_empty()).then(|| name.to_owned())
}

fn extension(name: &str) -> Option<&str> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty() && ext.len() <= 16 && ext.bytes().all(|b| b.is_ascii_alphanumeric()))
        .then_some(ext)
}

fn resolve_filename(browser_name: Option<&str>, server_name: Option<&str>, url: &Url, mime: Option<&str>) -> String {
    let url_name = url.path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|segment| percent_encoding::percent_decode_str(segment).decode_utf8().ok())
        .and_then(|name| clean_filename(&name));
    let url_name = url_name.as_deref().filter(|name| {
        !extension(name).is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "php" | "asp" | "aspx" | "jsp" | "cgi"))
    });
    let name = server_name
        .or(url_name.filter(|name| extension(name).is_some()))
        .or_else(|| browser_name.or(url_name).filter(|name| name.chars().count() <= 64))
        .unwrap_or("下载文件");
    let name = match (extension(name), mime.and_then(extension_for_mime)) {
        (None, Some(ext)) => format!("{name}.{ext}"),
        _ => name.to_owned(),
    };
    limit_filename(&name)
}

fn extension_for_mime(mime: &str) -> Option<&'static str> {
    Some(match mime.split(';').next()?.trim().to_ascii_lowercase().as_str() {
        "application/pdf" => "pdf",
        "application/zip" | "application/x-zip-compressed" => "zip",
        "application/gzip" | "application/x-gzip" => "gz",
        "application/x-7z-compressed" => "7z",
        "application/vnd.rar" | "application/x-rar-compressed" => "rar",
        "application/x-apple-diskimage" => "dmg",
        "application/vnd.android.package-archive" => "apk",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => "pptx",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "audio/mpeg" => "mp3",
        "video/mp4" => "mp4",
        "text/csv" => "csv",
        "text/plain" => "txt",
        _ => return None,
    })
}

fn limit_filename(name: &str) -> String {
    const MAX_BYTES: usize = 240;
    if name.len() <= MAX_BYTES {
        return name.to_owned();
    }
    let suffix = extension(name).map_or("", |ext| {
        let stem_end = name.len() - ext.len() - 1;
        let stem = &name[..stem_end];
        let start = if stem.to_ascii_lowercase().ends_with(".tar") { stem_end - 4 } else { stem_end };
        &name[start..]
    });
    let end = (0..=MAX_BYTES - suffix.len()).rev().find(|&i| name.is_char_boundary(i)).unwrap_or(0);
    format!("{}{suffix}", name[..end].trim_end())
}

pub fn present(app: &tauri::AppHandle, payload: super::QuickOpenPayload) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        // Show the main WebView first. The frontend owns the centered modal and
        // delays it after its DOM is available; no hidden WebView handshake.
        let _ = super::show_main_window(handle.clone());
        handle.state::<super::BrowserWakeState>().0.store(true, Ordering::Release);
        if let Ok(mut current) = handle.state::<super::QuickWindowState>().0.lock() {
            *current = payload.clone();
        }
        let state = handle.state::<BrowserDialogState>();
        let context = BrowserDialogContext { id: state.sequence.fetch_add(1, Ordering::AcqRel) + 1, payload };
        if let Ok(mut pending) = state.pending.lock() { *pending = Some(context.clone()); }
        let _ = handle.emit_to("main", "tdm://browser-download", context);
    });
}

#[tauri::command]
pub fn finish_browser_dialog(app: tauri::AppHandle, id: u64, accepted: bool) -> Result<(), String> {
    let state = app.state::<BrowserDialogState>();
    let mut pending = state.pending.lock().map_err(|error| error.to_string())?;
    let context = pending.as_ref().filter(|context| context.id == id)
        .ok_or("下载请求已更新，请确认新的下载链接")?;
    if let Some(request) = context.payload.request_id.clone() {
        super::browser_bridge::browser_download_decision(app.clone(), request, accepted)?;
    }
    *pending = None;
    app.state::<super::BrowserWakeState>().0.store(false, Ordering::Release);
    if let Ok(mut current) = app.state::<super::QuickWindowState>().0.lock() {
        *current = super::QuickOpenPayload::default();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_names_for_opaque_download_urls() {
        let token = "a".repeat(96);
        let url = Url::parse(&format!("https://example.com/{token}?signature=abc")).unwrap();
        let header = "attachment; filename=report; filename*=UTF-8'zh'%E6%8A%A5%E5%91%8A.pdf";
        let server_name = filename_from_cd(header).unwrap();
        assert_eq!(resolve_filename(Some(&token), Some(&server_name), &url, None), "报告.pdf");
        assert_eq!(resolve_filename(Some(&token), None, &url, Some("application/zip")), "下载文件.zip");
        assert_eq!(resolve_filename(None, None, &url, Some("application/octet-stream")), "下载文件");
    }

    #[test]
    fn preserves_download_extensions_and_removes_url_parameters() {
        let url = Url::parse("https://example.com/%E6%B5%8B%E8%AF%95.tar.gz?token=1").unwrap();
        assert_eq!(resolve_filename(None, None, &url, Some("application/octet-stream")), "测试.tar.gz");
        let url = Url::parse("https://example.com/download.php").unwrap();
        assert_eq!(resolve_filename(None, Some("report"), &url, Some("APPLICATION/PDF; charset=utf-8")), "report.pdf");
        let long_name = format!("{}.pdf", "报".repeat(100));
        let limited = limit_filename(&long_name);
        assert!(limited.len() <= 240);
        assert!(limited.ends_with(".pdf"));
    }
}
