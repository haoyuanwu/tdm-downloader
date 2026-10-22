//! Detect direct HTTP(S) downloads copied to the clipboard.

use crate::engine::{self, ClientConfig, DownloadEngine};
use crate::models::DownloadTask;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE, RANGE};
use std::sync::Arc;
use std::time::Duration;
use tauri::Manager;
use tauri_plugin_clipboard_manager::ClipboardExt;

#[derive(Clone, Debug)]
pub struct ClipboardCandidate {
    pub url: String,
    pub filename: String,
}

pub fn clipboard_url(text: &str) -> Option<String> {
    let text = text.trim();
    if text.chars().any(char::is_whitespace) {
        return None;
    }
    let url = url::Url::parse(text).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.host_str().is_some()).then(|| url.to_string())
}

fn known_file_extension(url: &url::Url) -> bool {
    let filename = url
        .path_segments()
        .and_then(|parts| parts.last())
        .unwrap_or("");
    let ext = filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase());
    matches!(
        ext.as_deref(),
        Some(
            "zip"
                | "7z"
                | "rar"
                | "tar"
                | "gz"
                | "bz2"
                | "xz"
                | "zst"
                | "pdf"
                | "dmg"
                | "pkg"
                | "iso"
                | "img"
                | "exe"
                | "msi"
                | "appx"
                | "deb"
                | "rpm"
                | "apk"
                | "ipa"
                | "mp4"
                | "mkv"
                | "mov"
                | "mp3"
                | "flac"
                | "wav"
                | "png"
                | "jpg"
                | "jpeg"
                | "webp"
                | "gif"
                | "csv"
                | "txt"
                | "epub"
                | "torrent"
        )
    )
}

fn looks_downloadable(url: &url::Url, headers: &reqwest::header::HeaderMap) -> bool {
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let attachment = headers
        .get(CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase().contains("attachment"))
        .unwrap_or(false);
    if attachment {
        return true;
    }
    if matches!(content_type.as_str(), "text/html" | "application/xhtml+xml")
        || content_type.contains("json")
        || content_type.contains("xml")
        || content_type.contains("javascript")
    {
        return false;
    }
    known_file_extension(url)
        || content_type.starts_with("application/")
        || content_type.starts_with("image/")
        || content_type.starts_with("audio/")
        || content_type.starts_with("video/")
}

pub(crate) async fn inspect_url(app: &tauri::AppHandle, url: &str) -> Option<ClipboardCandidate> {
    let parsed = url::Url::parse(url).ok()?;
    let engine = app.state::<Arc<DownloadEngine>>();
    let settings = engine.settings.lock().await.clone();
    let client = engine::build_task_client(&ClientConfig {
        proxy_mode: settings.proxy_mode,
        proxy_addr: settings.proxy_addr,
        user_agent: settings.user_agent,
        headers: Vec::new(),
    });
    let head = client
        .head(url)
        .timeout(Duration::from_secs(8))
        .send()
        .await;
    let response = match head {
        Ok(response) if response.status().is_success() => response,
        _ => client
            .get(url)
            .header(RANGE, "bytes=0-0")
            .timeout(Duration::from_secs(8))
            .send()
            .await
            .ok()?,
    };
    if !response.status().is_success() || !looks_downloadable(&parsed, response.headers()) {
        return None;
    }
    let filename = response
        .headers()
        .get(CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.split(';').find_map(|part| {
                let (key, value) = part.trim().split_once('=')?;
                (key.eq_ignore_ascii_case("filename") && !value.trim().is_empty())
                    .then(|| value.trim().trim_matches('"').to_string())
            })
        })
        .unwrap_or_else(|| engine::filename_from_url(url));
    Some(ClipboardCandidate {
        url: url.to_string(),
        filename,
    })
}

pub async fn monitor(app: tauri::AppHandle) {
    let mut last_text = app.clipboard().read_text().unwrap_or_default();
    loop {
        tokio::time::sleep(Duration::from_millis(1400)).await;
        let Ok(text) = app.clipboard().read_text() else {
            continue;
        };
        if text == last_text {
            continue;
        }
        last_text = text.clone();
        crate::clear_clipboard_candidate(&app);
        let Some(url) = clipboard_url(&text) else {
            continue;
        };
        let tasks: Vec<DownloadTask> = app.state::<Arc<DownloadEngine>>().snapshot().await;
        if tasks.iter().any(|task| task.url == url) {
            continue;
        }
        let Some(candidate) = inspect_url(&app, &url).await else {
            continue;
        };
        if app.clipboard().read_text().ok().as_deref() != Some(text.as_str()) {
            continue;
        }
        crate::show_clipboard_candidate(&app, candidate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_single_web_urls_are_candidates() {
        assert!(clipboard_url("https://example.com/file.zip").is_some());
        assert!(clipboard_url("https://example.com\nhttps://other.com").is_none());
        assert!(clipboard_url("file:///tmp/file.zip").is_none());
    }

    #[test]
    fn pages_do_not_prompt() {
        let url = url::Url::parse("https://example.com/file.zip").unwrap();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(CONTENT_TYPE, "text/html".parse().unwrap());
        assert!(!looks_downloadable(&url, &headers));
        headers.insert(CONTENT_TYPE, "application/ld+json".parse().unwrap());
        assert!(!looks_downloadable(&url, &headers));
        headers.insert(CONTENT_TYPE, "application/zip".parse().unwrap());
        assert!(looks_downloadable(&url, &headers));
        headers.insert(CONTENT_TYPE, "text/html".parse().unwrap());
        headers.insert(
            CONTENT_DISPOSITION,
            "attachment; filename=file.html".parse().unwrap(),
        );
        assert!(looks_downloadable(&url, &headers));
    }
}
