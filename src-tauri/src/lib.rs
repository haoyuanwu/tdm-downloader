//! TDM Downloader —— 使用 Tauri 2 + Rust 构建的多线程下载管理器
//!
//! 功能对标 TDM Fast（已逆向分析），但代码与 UI 完全自研。

pub mod engine;
pub mod models;

use engine::DownloadEngine;
use models::{AppInfo, DownloadTask, NewTaskRequest, ProbeInfo, Settings};
use std::sync::Arc;
use tauri::{Emitter, Manager};

/// 托盘「新建下载」通知前端的事件名
pub const EVT_NEW_TASK: &str = "tdm://new-task";

/// 新建下载任务
#[tauri::command]
async fn start_download(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<DownloadEngine>>,
    req: NewTaskRequest,
) -> Result<DownloadTask, String> {
    let engine = Arc::clone(&state);
    let task = engine::spawn_task(app.clone(), engine.clone(), req).await?;
    let list = engine.snapshot().await;
    let _ = app.emit(engine::EVT_TASKS, list);
    Ok(task)
}

/// 列出所有任务
#[tauri::command]
async fn list_tasks(state: tauri::State<'_, Arc<DownloadEngine>>) -> Result<Vec<DownloadTask>, String> {
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
    Ok(state.settings.lock().await.clone())
}

/// 保存设置
#[tauri::command]
async fn save_settings(
    state: tauri::State<'_, Arc<DownloadEngine>>,
    settings: Settings,
) -> Result<(), String> {
    {
        let mut s = state.settings.lock().await;
        *s = settings.clone();
    }
    engine::persist_settings(&settings);
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
    #[cfg(not(target_os = "windows"))]
    {
        let _ = target;
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
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    Ok(())
}

/// 隐藏主窗口到托盘
#[tauri::command]
fn hide_main_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    Ok(())
}

/// 退出程序
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// 读取 / 设置开机自启（Windows 启动目录）
#[tauri::command]
fn set_autostart(enabled: bool) -> Result<(), String> {
    let p = autostart_file()?;
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let content = format!("@echo off\r\nstart \"\" \"{}\"\r\n", exe.display());
        std::fs::write(&p, content).map_err(|e| e.to_string())
    } else if p.exists() {
        std::fs::remove_file(&p).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
fn get_autostart() -> bool {
    autostart_file().map(|p| p.exists()).unwrap_or(false)
}

/// Windows 启动目录脚本路径
fn autostart_file() -> Result<std::path::PathBuf, String> {
    let appdata = std::env::var("APPDATA").map_err(|_| "未找到 APPDATA 目录".to_string())?;
    Ok(std::path::PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup")
        .join("TDM Downloader.cmd"))
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
    app.clipboard()
        .write_text(text)
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let engine = Arc::new(DownloadEngine::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(engine)
        .invoke_handler(tauri::generate_handler![
            start_download,
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
        ])
        .setup(|app| {
            build_tray(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // 关闭窗口时按设置最小化到托盘，而不是退出
            if let tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } = event
            {
                if label == "main" {
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
                    }
                }
            }
        });
}

/// 构建系统托盘图标与菜单
fn build_tray(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
    use tauri::tray::TrayIconEvent;

    let show = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
    let new = MenuItemBuilder::with_id("new", "新建下载").build(app)?;
    let pause = MenuItemBuilder::with_id("pause_all", "暂停全部").build(app)?;
    let resume = MenuItemBuilder::with_id("resume_all", "恢复全部").build(app)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;

    let menu = MenuBuilder::new(app)
        .items(&[&show, &new, &pause, &resume, &sep, &quit])
        .build()?;

    let handle = app.handle().clone();
    app.on_menu_event(move |app, event| {
        let engine = app.state::<Arc<DownloadEngine>>().inner().clone();
        match event.id().as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.unminimize();
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "new" => {
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
        let _ = tray.set_menu(Some(menu));
        tray.on_tray_icon_event(move |_tray, e| {
            if let TrayIconEvent::Click { .. } = e {
                if let Some(w) = handle.get_webview_window("main") {
                    if w.is_visible().unwrap_or(false) {
                        let _ = w.hide();
                    } else {
                        let _ = w.unminimize();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            }
        });
    }

    Ok(())
}
