use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
use tauri::{Emitter, Manager};

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
