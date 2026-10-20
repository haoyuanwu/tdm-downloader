//! TDM Downloader —— 使用 Tauri 2 + Rust 构建的多线程下载管理器
//!
//! 功能对标 TDM Fast（已逆向分析），但代码与 UI 完全自研。

pub mod engine;
pub mod models;

use engine::DownloadEngine;
use models::{DownloadTask, NewTaskRequest, Settings};
use std::sync::Arc;
use tauri::Emitter;

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
        ])
        .setup(|app| {
            // 系统托盘（在 tauri.conf.json 中已声明 id = "main"）
            if let Some(tray) = app.tray_by_id("main") {
                let _ = tray.set_tooltip(Some("TDM Downloader"));
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {});
}
