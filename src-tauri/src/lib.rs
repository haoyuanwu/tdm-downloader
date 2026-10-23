//! TDM Downloader —— 使用 Tauri 2 + Rust 构建的多线程下载管理器
//!
//! 功能对标 TDM Fast（已逆向分析），但代码与 UI 完全自研。

mod clipboard;
mod browser_bridge;
mod safari_extension;
mod extension_manager;
pub mod engine;
pub mod models;
mod notifications;
mod popup_position;
mod tray_progress;

use engine::DownloadEngine;
use models::{AppInfo, DownloadTask, NewTaskRequest, ProbeInfo, Settings};
use std::sync::Arc;
use tauri::{Emitter, Manager};

/// 托盘「新建下载」通知前端的事件名
pub const EVT_NEW_TASK: &str = "tdm://new-task";
pub const EVT_QUICK_OPEN: &str = "tdm://quick-open";

#[derive(Default)]
struct ClipboardState(std::sync::Mutex<Option<clipboard::ClipboardCandidate>>);

#[derive(Default)]
struct BrowserWakeState(std::sync::atomic::AtomicBool);

#[derive(Clone, serde::Serialize, Default)]
struct QuickOpenPayload {
    mode: String,
    url: Option<String>,
    filename: Option<String>,
    request_id: Option<String>,
}

#[derive(Default)]
struct QuickWindowState(
    std::sync::Mutex<QuickOpenPayload>,
    std::sync::Mutex<Option<popup_position::Rect>>,
    std::sync::atomic::AtomicBool,
    std::sync::atomic::AtomicBool,
);

/// 前端提交了实际内容后才显示原生窗口，冷启动时不会展示未加载的 WebView。
#[tauri::command]
fn quick_window_ready(app: tauri::AppHandle) {
    trace_quick_window(&app, "frontend-ready");
    let state = app.state::<QuickWindowState>();
    state.2.store(true, std::sync::atomic::Ordering::Release);
    if state.3.load(std::sync::atomic::Ordering::Acquire) {
        if let Some(window) = app.get_webview_window("quick") {
            #[cfg(target_os = "macos")]
            let _ = app.show();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

#[tauri::command]
fn frontend_health(app: tauri::AppHandle, label: String, message: String) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(std::env::temp_dir().join("tdm-webview-health.log")) {
        let native = app.get_webview_window(&label).map(|window| format!(
            " visible={} focused={}", window.is_visible().unwrap_or(false), window.is_focused().unwrap_or(false))).unwrap_or_default();
        let _ = writeln!(file, "{}: {}{}", label, message.chars().take(1200).collect::<String>(), native);
    }
}

fn trace_quick_window(app: &tauri::AppHandle, event: &str) {
    let state = app.state::<QuickWindowState>();
    let browser = app.state::<BrowserWakeState>().0.load(std::sync::atomic::Ordering::Acquire);
    let ready = state.2.load(std::sync::atomic::Ordering::Acquire);
    let requested = state.3.load(std::sync::atomic::Ordering::Acquire);
    frontend_health(app.clone(), "quick".into(), format!("{event}: browser={browser} ready={ready} requested={requested}"));
}

/// 新建下载任务
#[tauri::command]
async fn start_download(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<DownloadEngine>>,
    req: NewTaskRequest,
) -> Result<DownloadTask, String> {
    let engine = Arc::clone(&state);
    let task = engine::spawn_task(app.clone(), engine.clone(), req).await?;
    let should_clear = app
        .state::<ClipboardState>()
        .0
        .lock()
        .ok()
        .and_then(|candidate| candidate.as_ref().map(|c| c.url == task.url))
        .unwrap_or(false);
    if should_clear {
        clear_clipboard_candidate(&app);
    }
    let list = engine.snapshot().await;
    let _ = app.emit(engine::EVT_TASKS, list);
    Ok(task)
}

/// 列出所有任务
#[tauri::command]
async fn list_tasks(
    state: tauri::State<'_, Arc<DownloadEngine>>,
) -> Result<Vec<DownloadTask>, String> {
    Ok(state.snapshot().await)
}

/// 暂停任务
#[tauri::command]
async fn pause_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<DownloadEngine>>,
    id: String,
) -> Result<(), String> {
    let engine = Arc::clone(&state);
    engine::pause_task(&engine, &id).await?;
    let list = engine.snapshot().await;
    let _ = app.emit(engine::EVT_TASKS, list);
    Ok(())
}

/// 恢复任务
#[tauri::command]
async fn resume_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<DownloadEngine>>,
    id: String,
) -> Result<(), String> {
    let engine = Arc::clone(&state);
    engine::resume_task(app.clone(), engine.clone(), &id).await?;
    let list = engine.snapshot().await;
    let _ = app.emit(engine::EVT_TASKS, list);
    Ok(())
}

/// 取消任务
#[tauri::command]
async fn cancel_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<DownloadEngine>>,
    id: String,
) -> Result<(), String> {
    let engine = Arc::clone(&state);
    engine::cancel_task(&engine, &id).await?;
    let list = engine.snapshot().await;
    let _ = app.emit(engine::EVT_TASKS, list);
    Ok(())
}

/// 删除任务
#[tauri::command]
async fn remove_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<DownloadEngine>>,
    id: String,
    delete_file: Option<bool>,
) -> Result<(), String> {
    let engine = Arc::clone(&state);
    engine::remove_task(&engine, &id, delete_file.unwrap_or(false)).await?;
    let list = engine.snapshot().await;
    let _ = app.emit(engine::EVT_TASKS, list);
    Ok(())
}

/// 清空已完成任务
#[tauri::command]
async fn clear_completed(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<DownloadEngine>>,
) -> Result<(), String> {
    let engine = Arc::clone(&state);
    let ids: Vec<String> = {
        let list = engine.snapshot().await;
        list.into_iter()
            .filter(|t| t.status == models::TaskStatus::Completed)
            .map(|t| t.id)
            .collect()
    };
    for id in ids {
        let _ = engine::remove_task(&engine, &id, false).await;
    }
    let list = engine.snapshot().await;
    let _ = app.emit(engine::EVT_TASKS, list);
    Ok(())
}

/// 获取设置
#[tauri::command]
async fn get_settings(state: tauri::State<'_, Arc<DownloadEngine>>) -> Result<Settings, String> {
    let mut settings = state.settings.lock().await.clone();
    settings.autostart = get_autostart()?;
    Ok(settings)
}

/// 保存设置
#[tauri::command]
async fn save_settings(
    state: tauri::State<'_, Arc<DownloadEngine>>,
    settings: Settings,
) -> Result<(), String> {
    {
        let mut s = state.settings.lock().await;
        let previous_startup = get_autostart()?;
        set_autostart(settings.autostart)?;
        if let Err(error) = engine::persist_settings(&settings) {
            let _ = set_autostart(previous_startup);
            return Err(error);
        }
        *s = settings.clone();
    }
    Ok(())
}

/// 在资源管理器中打开下载目录
#[tauri::command]
async fn open_download_dir(path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(&path);
    let target = if p.is_dir() {
        p
    } else {
        p.parent().map(|x| x.to_path_buf()).unwrap_or(p)
    };
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(target)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(target)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 选择保存目录（调用系统文件对话框）
#[tauri::command]
async fn pick_directory(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().pick_folder(move |folder| {
        let val = folder
            .map(|f| f.to_string())
            .filter(|s| !s.trim().is_empty());
        let _ = tx.send(val);
    });
    rx.recv().map_err(|e| e.to_string())
}

/// 打开文件或目录（用系统默认程序）
#[tauri::command]
async fn open_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

// ────────────────────────── 新增：预解析 / 批量操作 / 系统集成 ──────────────────────────

/// URL 预解析：在新建下载对话框里提前拿到文件名、大小、是否支持断点续传
#[tauri::command]
async fn probe_url(
    state: tauri::State<'_, Arc<DownloadEngine>>,
    url: String,
    user_agent: Option<String>,
    headers: Option<Vec<(String, String)>>,
) -> Result<ProbeInfo, String> {
    let cfg = {
        let s = state.settings.lock().await;
        engine::ClientConfig {
            proxy_mode: s.proxy_mode,
            proxy_addr: s.proxy_addr.clone(),
            user_agent: user_agent.unwrap_or_default(),
            headers: headers.unwrap_or_default(),
        }
    };
    let client = engine::build_task_client(&cfg);
    match engine::probe(&client, &url).await {
        Ok(p) => Ok(ProbeInfo {
            filename: p.filename.unwrap_or_default(),
            total_size: p.total_size,
            accept_ranges: p.accept_ranges,
            ok: true,
            error: String::new(),
        }),
        Err(e) => Ok(ProbeInfo {
            filename: String::new(),
            total_size: 0,
            accept_ranges: false,
            ok: false,
            error: e,
        }),
    }
}

/// 暂停全部进行中的任务
#[tauri::command]
async fn pause_all(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<DownloadEngine>>,
) -> Result<(), String> {
    engine::pause_all(&app, &Arc::clone(&state)).await;
    Ok(())
}

/// 恢复全部暂停/失败的任务
#[tauri::command]
async fn resume_all(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<DownloadEngine>>,
) -> Result<(), String> {
    engine::resume_all(&app, &Arc::clone(&state)).await;
    Ok(())
}

/// 任务栏进度条：progress 为 0.0~1.0，None 表示清除
#[tauri::command]
fn set_progress_bar(window: tauri::WebviewWindow, progress: Option<f64>) -> Result<(), String> {
    use tauri::window::{ProgressBarState, ProgressBarStatus};
    let state = match progress {
        None => ProgressBarState {
            status: Some(ProgressBarStatus::None),
            progress: None,
        },
        Some(p) => ProgressBarState {
            status: Some(ProgressBarStatus::Normal),
            progress: Some((p.clamp(0.0, 1.0) * 100.0).round() as u64),
        },
    };
    window.set_progress_bar(state).map_err(|e| e.to_string())
}

/// 显示并聚焦主窗口（托盘菜单用）
#[tauri::command]
fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    trace_quick_window(&app, "show-main");
    let _ = hide_quick_window(app.clone());
    show_dock(&app);
    if let Some(w) = app.get_webview_window("main") {
        w.unminimize().map_err(|error| error.to_string())?;
        w.show().map_err(|error| error.to_string())?;
        #[cfg(target_os = "macos")]
        app.show().map_err(|error| error.to_string())?;
        w.set_focus().map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// 隐藏主窗口到托盘
#[tauri::command]
fn hide_main_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    hide_dock(&app);
    Ok(())
}

#[tauri::command]
async fn quick_clipboard_hint(app: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let text = app.clipboard().read_text().ok()?;
    let candidate = app
        .state::<ClipboardState>()
        .0
        .lock()
        .ok()
        .and_then(|c| c.clone());
    if let Some(candidate) = candidate {
        if clipboard::clipboard_url(&text).as_deref() == Some(candidate.url.as_str()) {
            return Some(candidate.url);
        }
    }
    let url = clipboard::clipboard_url(&text)?;
    clipboard::inspect_url(&app, &url).await.map(|c| c.url)
}

#[tauri::command]
fn quick_open_context(app: tauri::AppHandle) -> QuickOpenPayload {
    app.state::<QuickWindowState>()
        .0
        .lock()
        .map(|state| state.clone())
        .unwrap_or_default()
}

#[tauri::command]
fn show_quick_input(app: tauri::AppHandle) {
    show_quick_window(&app, None);
}

#[tauri::command]
fn hide_quick_window(app: tauri::AppHandle) -> Result<(), String> {
    trace_quick_window(&app, "hide");
    app.state::<BrowserWakeState>().0.store(false, std::sync::atomic::Ordering::Release);
    app.state::<QuickWindowState>().3.store(false, std::sync::atomic::Ordering::Release);
    let pending = app.state::<QuickWindowState>().0.lock().ok().and_then(|state| state.request_id.clone());
    if let Some(id) = pending { let _ = browser_bridge::browser_download_decision(app.clone(), id, false); }
    if let Some(w) = app.get_webview_window("quick") {
        w.hide().map_err(|e| e.to_string())?;
    }
    let clipboard_mode = app
        .state::<QuickWindowState>()
        .0
        .lock()
        .map(|current| current.mode == "clipboard")
        .unwrap_or(false);
    if clipboard_mode {
        clear_clipboard_candidate(&app);
    }
    Ok(())
}

/// 退出程序
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// 读取 / 设置登录时启动
#[tauri::command]
fn set_autostart(enabled: bool) -> Result<(), String> {
    let p = autostart_file()?;
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        #[cfg(target_os = "windows")]
        {
            let content = format!("@echo off\r\nstart \"\" \"{}\" --autostart\r\n", exe.display());
            return std::fs::write(&p, content).map_err(|e| e.to_string());
        }
        #[cfg(target_os = "macos")]
        {
            let escaped = exe
                .to_string_lossy()
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
                .replace('\'', "&apos;");
            let content = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>com.tdm.downloader</string><key>ProgramArguments</key><array><string>{escaped}</string><string>--autostart</string></array><key>RunAtLoad</key><true/></dict></plist>\n");
            std::fs::write(&p, content).map_err(|e| e.to_string())?;
            // Login agents are loaded automatically on the next login.
            return Ok(());
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        return Err("当前平台不支持开机自启".to_string());
    } else if p.exists() {
        std::fs::remove_file(&p).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
fn get_autostart() -> Result<bool, String> {
    Ok(autostart_file()?.exists())
}

/// 平台对应的登录启动项路径
fn autostart_file() -> Result<std::path::PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").map_err(|_| "未找到 HOME 目录".to_string())?;
        let dir = std::path::PathBuf::from(home).join("Library/LaunchAgents");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        return Ok(dir.join("com.tdm.downloader.plist"));
    }
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").map_err(|_| "未找到 APPDATA 目录".to_string())?;
        return Ok(std::path::PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup")
            .join("TDM Downloader.cmd"));
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    Err("当前平台不支持开机自启".to_string())
}

/// 应用信息（关于）
#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        exe_path: std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    }
}

/// 写入剪贴板
#[tauri::command]
async fn copy_text(app: tauri::AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let engine = Arc::new(DownloadEngine::new());

    tauri::Builder::default()
        .on_page_load(|webview, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                let label = webview.label().to_string();
                let code = format!(r#"setTimeout(() => window.__TAURI_INTERNALS__.invoke('frontend_health', {{label: {label:?}, message: JSON.stringify({{path:location.pathname, scripts:[...document.scripts].map(s=>s.getAttribute('src')), root:document.getElementById('root')?.innerText?.slice(0,350), boot:window.__tdmBootError || null}})}}).catch(()=>{{}}), 800);"#);
                let _ = webview.eval(code);
            }
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .manage(engine)
        .manage(ClipboardState::default())
        .manage(BrowserWakeState::default())
        .manage(QuickWindowState::default())
        .manage(tray_progress::TrayProgressState::default())
        .manage(notifications::NotificationState::default())
        .invoke_handler(tauri::generate_handler![
            start_download,
            frontend_health,
            list_tasks,
            pause_task,
            resume_task,
            cancel_task,
            remove_task,
            clear_completed,
            get_settings,
            save_settings,
            open_download_dir,
            pick_directory,
            open_path,
            probe_url,
            pause_all,
            resume_all,
            set_progress_bar,
            show_main_window,
            hide_main_window,
            quit_app,
            set_autostart,
            get_autostart,
            get_app_info,
            copy_text,
            quick_clipboard_hint,
            quick_open_context,
            quick_window_ready,
            show_quick_input,
            hide_quick_window,
            browser_bridge::browser_download_decision,
            safari_extension::open_safari_connector,
            extension_manager::browser_extension_status,
            extension_manager::prepare_chrome_extension,
            extension_manager::configure_chrome_extension,
            extension_manager::open_chrome_extension_directory,
            notifications::notification_status,
            notifications::test_download_notification,
            notifications::open_notification_settings,
        ])
        .setup(|app| {
            extension_manager::refresh_app_path();
            safari_extension::refresh_registered_copy();
            tauri::WebviewWindowBuilder::new(
                app,
                "quick",
                tauri::WebviewUrl::App("quick.html".into()),
            )
            .title("快速下载")
            .inner_size(380.0, 174.0)
            .resizable(false)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .build()?;
            build_tray(app)?;
            let login_launch = std::env::args().any(|argument| argument == "--autostart");
            let background = app.state::<Arc<DownloadEngine>>().settings.try_lock()
                .map(|settings| settings.startup_hidden).unwrap_or(true);
            if login_launch && background {
                if let Some(window) = app.get_webview_window("main") { let _ = window.hide(); }
                hide_dock(app.handle());
            }
            tauri::async_runtime::spawn(clipboard::monitor(app.handle().clone()));
            tauri::async_runtime::spawn(tray_progress::monitor(app.handle().clone()));
            tauri::async_runtime::spawn(notifications::initialize(app.handle().clone()));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = &event {
                // Opening a URL also activates/reopens the app. Do not interpret
                // that activation as a request to dismiss its download dialog.
                if app.state::<BrowserWakeState>().0.load(std::sync::atomic::Ordering::Acquire) {
                    let state = app.state::<QuickWindowState>();
                    if state.2.load(std::sync::atomic::Ordering::Acquire)
                        && state.3.load(std::sync::atomic::Ordering::Acquire) {
                        if let Some(window) = app.get_webview_window("quick") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                } else {
                    let _ = show_main_window(app.clone());
                }
            }
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = &event {
                for url in urls { browser_bridge::receive(app, url); }
            }
            // 关闭窗口时按设置最小化到托盘，而不是退出
            if let tauri::RunEvent::WindowEvent { label, event, .. } = event {
                if label == "quick" {
                    match event {
                        tauri::WindowEvent::CloseRequested { api, .. } => {
                            api.prevent_close();
                            let _ = hide_quick_window(app.clone());
                        }
                        tauri::WindowEvent::Focused(false) => {
                            // Browser confirmation requires an explicit choice;
                            // activation races or switching apps must not cancel it.
                            if !app.state::<BrowserWakeState>().0.load(std::sync::atomic::Ordering::Acquire) {
                                let _ = hide_quick_window(app.clone());
                            }
                        }
                        _ => {}
                    }
                } else if label == "main" {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let to_tray = app
                            .state::<Arc<DownloadEngine>>()
                            .settings
                            .try_lock()
                            .map(|s| s.close_to_tray)
                            .unwrap_or(true);
                        if to_tray {
                            api.prevent_close();
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.hide();
                            }
                            hide_dock(app);
                        }
                    }
                }
            }
        });
}

/// 构建系统托盘图标与菜单
fn build_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};

    let handle = app.handle().clone();
    app.on_menu_event(move |app, event| {
        let engine = app.state::<Arc<DownloadEngine>>().inner().clone();
        match event.id().as_ref() {
            "show" => {
                let _ = hide_quick_window(app.clone());
                show_dock(app);
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.unminimize();
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "new" => {
                let _ = hide_quick_window(app.clone());
                show_dock(app);
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.unminimize();
                    let _ = w.show();
                    let _ = w.set_focus();
                }
                let _ = app.emit(EVT_NEW_TASK, ());
            }
            "pause_all" => {
                let app_c = app.clone();
                tauri::async_runtime::spawn(async move {
                    engine::pause_all(&app_c, &engine).await;
                });
            }
            "resume_all" => {
                let app_c = app.clone();
                tauri::async_runtime::spawn(async move {
                    engine::resume_all(&app_c, &engine).await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        }
    });

    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some("TDM Downloader"));
        let _ = tray.set_title(None::<&str>);
        refresh_tray_menu(app.handle())?;
        // Tauri 默认左键也会弹出托盘菜单（tray-icon 默认 menu_on_left_click = true），
        // 会和「左键切换主窗口」打架：菜单刚弹出就被抢焦点的窗口挤掉。
        // 关掉它，约定：左键 = 显示/隐藏窗口，右键 = 弹出菜单。
        let _ = tray.set_show_menu_on_left_click(false);
        tray.on_tray_icon_event(move |_tray, e| {
            // 左键打开下载列表或快速输入，右键保留任务管理菜单。
            // 右键点击由系统弹出托盘菜单，此时绝不能 show/set_focus 主窗口，
            // 否则主窗口会抢走焦点，导致刚弹出的菜单瞬间消失（表现为闪一下就没了）。
            let clicked_rect = match e {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    rect,
                    ..
                }
                | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    rect,
                    ..
                } => rect,
                _ => return,
            };
            // 使用本次事件的图标矩形，避免 tray.rect() 返回另一屏菜单栏的缓存位置。
            let anchor = popup_position::resolve(&handle, Some(clicked_rect));
            toggle_quick_window(&handle, anchor);
        });
    }

    Ok(())
}

fn refresh_tray_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::MenuBuilder;

    let menu = MenuBuilder::new(app)
        .text("show", "显示主窗口")
        .text("new", "新建下载")
        .text("pause_all", "暂停全部")
        .text("resume_all", "恢复全部")
        .separator()
        .text("quit", "退出")
        .build()?;
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

fn show_clipboard_candidate(app: &tauri::AppHandle, candidate: clipboard::ClipboardCandidate) {
    if let Ok(mut current) = app.state::<ClipboardState>().0.lock() {
        *current = Some(candidate.clone());
    }
    show_quick_window(app, Some(candidate));
}

fn toggle_quick_window(app: &tauri::AppHandle, anchor: Option<popup_position::Anchor>) {
    if let Some(w) = app.get_webview_window("quick") {
        let show_tasks = app
            .state::<tray_progress::TrayProgressState>()
            .has_tasks
            .load(std::sync::atomic::Ordering::Relaxed);
        let current_mode = app
            .state::<QuickWindowState>()
            .0
            .lock()
            .map(|current| current.mode.clone())
            .unwrap_or_default();
        let already_open = if show_tasks {
            current_mode == "tasks"
        } else {
            current_mode == "manual"
        };
        let same_screen = app
            .state::<QuickWindowState>()
            .1
            .lock()
            .map(|previous| anchor.map(|a| Some(a.bounds) == *previous).unwrap_or(true))
            .unwrap_or(false);
        if w.is_visible().unwrap_or(false) && already_open && same_screen {
            let _ = hide_quick_window(app.clone());
        } else if show_tasks {
            present_quick_window_at(
                app,
                QuickOpenPayload {
                    mode: "tasks".into(),
                    ..Default::default()
                },
                anchor,
            );
        } else {
            present_quick_window_at(
                app,
                QuickOpenPayload {
                    mode: "manual".into(),
                    ..Default::default()
                },
                anchor,
            );
        }
    }
}

fn show_quick_window(app: &tauri::AppHandle, candidate: Option<clipboard::ClipboardCandidate>) {
    let payload = match candidate {
        Some(candidate) => QuickOpenPayload {
            mode: "clipboard".to_string(),
            url: Some(candidate.url),
            filename: Some(candidate.filename),
            request_id: None,
        },
        None => QuickOpenPayload {
            mode: "manual".to_string(),
            ..Default::default()
        },
    };
    present_quick_window(app, payload);
}

fn present_quick_window(app: &tauri::AppHandle, payload: QuickOpenPayload) {
    let handle = app.clone();
    // 剪贴板监控来自后台任务，AppKit 屏幕查询及窗口定位统一放到主线程。
    let _ = app.run_on_main_thread(move || {
        let anchor = popup_position::resolve(&handle, None);
        present_quick_window_at(&handle, payload, anchor);
    });
}

fn present_quick_window_at(
    app: &tauri::AppHandle,
    payload: QuickOpenPayload,
    anchor: Option<popup_position::Anchor>,
) {
    let browser = app.state::<BrowserWakeState>();
    if payload.mode == "clipboard" && browser.0.load(std::sync::atomic::Ordering::Acquire) {
        return;
    }
    browser.0.store(payload.mode == "browser", std::sync::atomic::Ordering::Release);
    let pending = app.state::<QuickWindowState>().0.lock().ok().and_then(|state| state.request_id.clone());
    if let Some(id) = pending { let _ = browser_bridge::browser_download_decision(app.clone(), id, false); }
    if let Ok(mut current) = app.state::<QuickWindowState>().0.lock() {
        *current = payload.clone();
    }
    let Some(w) = app.get_webview_window("quick") else {
        return;
    };
    let height = if payload.mode == "tasks" {
        390.0
    } else {
        174.0
    };
    if let Some(anchor) = anchor {
        popup_position::apply(&w, anchor, height);
        if let Ok(mut previous) = app.state::<QuickWindowState>().1.lock() {
            *previous = Some(anchor.bounds);
        }
    } else {
        let _ = w.set_size(tauri::LogicalSize::new(380.0, height));
    }
    let state = app.state::<QuickWindowState>();
    state.2.store(false, std::sync::atomic::Ordering::Release);
    state.3.store(true, std::sync::atomic::Ordering::Release);
    trace_quick_window(app, "deliver-context");
    // 始终先交付数据；前端完成渲染后通过 quick_window_ready 显示。
    let _ = app.emit_to("quick", EVT_QUICK_OPEN, payload);
}

fn hide_dock(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_dock_visibility(false);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

fn show_dock(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_dock_visibility(true);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

fn clear_clipboard_candidate(app: &tauri::AppHandle) {
    if let Ok(mut current) = app.state::<ClipboardState>().0.lock() {
        *current = None;
    }
}
