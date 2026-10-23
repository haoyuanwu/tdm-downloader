use tauri::Manager;

/// Resolve only our own connector; callers cannot supply executable paths.
#[tauri::command]
pub async fn open_safari_connector(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut candidates = Vec::new();
        if let Ok(resources) = app.path().resource_dir() {
            candidates.push(resources.join("safari/TDM Safari Connector.app"));
        }
        #[cfg(debug_assertions)]
        candidates.push(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../release/safari/TDM Safari Connector.app"));
        let connector = candidates.into_iter().find(|path| path.join("Contents/MacOS/TDM Safari Connector").is_file())
            .ok_or("未找到 Safari 扩展安装器，请使用包含 Safari 扩展的新版 macOS 安装包。")?;
        let output = tauri::async_runtime::spawn_blocking(move || {
            std::process::Command::new("/usr/bin/open").arg(connector).output()
        }).await.map_err(|error| error.to_string())?
            .map_err(|error| format!("无法打开 Safari 扩展安装器：{error}"))?;
        if !output.status.success() {
            return Err(format!("无法打开 Safari 扩展安装器：{}", String::from_utf8_lossy(&output.stderr)));
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("Safari 扩展仅支持 macOS。".into())
    }
}
