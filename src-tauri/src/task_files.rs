use std::sync::Arc;
use tauri::Manager;
use crate::engine::DownloadEngine;

#[derive(Default)]
pub struct QuickInteraction(pub std::sync::atomic::AtomicBool);

#[tauri::command]
pub fn quick_task_interaction(app: tauri::AppHandle, active: bool) {
    app.state::<QuickInteraction>().0.store(active, std::sync::atomic::Ordering::Release);
}

#[tauri::command]
pub async fn reveal_task_file(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let task = app.state::<Arc<DownloadEngine>>().get(&id).await.ok_or("任务已不存在")?;
    let path = std::path::PathBuf::from(task.save_path);
    if path.exists() {
        tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
    } else {
        let parent = path.parent().ok_or("文件路径无效")?;
        use tauri_plugin_opener::OpenerExt;
        app.opener().open_path(parent.to_string_lossy().into_owned(), None::<&str>).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn save_task_as(app: tauri::AppHandle, id: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let task = app.state::<Arc<DownloadEngine>>().get(&id).await.ok_or("任务已不存在")?;
    if task.status != crate::models::TaskStatus::Completed { return Err("请等待下载完成后再另存为".into()); }
    tauri::async_runtime::spawn_blocking(move || {
        let source = std::path::PathBuf::from(task.save_path);
        if !source.is_file() { return Err("本地文件已被移动或删除".into()); }
        let mut chooser = app.dialog().file().set_title("另存为").set_file_name(&task.filename);
        if let Some(window) = app.get_webview_window("quick") { chooser = chooser.set_parent(&window); }
        let Some(file) = chooser.blocking_save_file() else { return Ok(None); };
        let target = file.into_path().map_err(|e| e.to_string())?;
        if source.canonicalize().ok() == target.canonicalize().ok() && target.exists() {
            return Err("另存位置与原文件相同，请选择其他位置或文件名".into());
        }
        std::fs::copy(&source, &target).map_err(|e| format!("另存为失败：{e}"))?;
        Ok(Some(target.to_string_lossy().into_owned()))
    }).await.map_err(|e| e.to_string())?
}
