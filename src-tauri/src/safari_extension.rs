#[cfg(target_os = "macos")]
use tauri::Manager;

#[cfg(target_os = "macos")]
fn installed_connector() -> Result<std::path::PathBuf, String> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
        .map(|home| home.join("Applications/TDM Safari Connector.app"))
        .ok_or("无法读取用户应用目录".into())
}

#[cfg(target_os = "macos")]
fn bundle_value(path: &std::path::Path, key: &str) -> Option<String> {
    let output = std::process::Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", &format!("Print :{key}")]).arg(path.join("Contents/Info.plist")).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(target_os = "macos")]
fn signing_team(path: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("/usr/bin/codesign").arg("-dvv").arg(path).output().ok()?;
    String::from_utf8_lossy(&output.stderr).lines()
        .find_map(|line| line.strip_prefix("TeamIdentifier="))
        .filter(|team| *team != "not set").map(str::to_owned)
}

#[cfg(target_os = "macos")]
fn register_single_copy(connector: &std::path::Path) -> Result<(), String> {
    let extension = connector.join("Contents/PlugIns/TDM Safari Connector Extension.appex");
    let listing = std::process::Command::new("/usr/bin/pluginkit")
        .args(["-m", "-A", "-D", "-v", "-i", "com.tdm.downloader.safari.Extension"])
        .output().map_err(|error| error.to_string())?;
    // Only unregister copies of our exact extension ID; never alter other extensions.
    for line in String::from_utf8_lossy(&listing.stdout).lines() {
        if let Some(path) = line.split('\t').next_back().filter(|path| path.starts_with('/')) {
            if std::path::Path::new(path) != extension {
                let output = std::process::Command::new("/usr/bin/pluginkit").args(["-r", path])
                    .output().map_err(|error| error.to_string())?;
                if !output.status.success() { return Err("无法清理 TDM 旧扩展注册，请关闭 Safari 后重试。".into()); }
            }
        }
    }
    let output = std::process::Command::new("/usr/bin/pluginkit").arg("-a").arg(extension)
        .output().map_err(|error| error.to_string())?;
    if !output.status.success() { return Err(format!("无法注册 Safari 扩展：{}", String::from_utf8_lossy(&output.stderr))); }
    Ok(())
}

/// App updates can register the bundled copy again; keep the user's fixed copy.
pub fn refresh_registered_copy() {
    #[cfg(target_os = "macos")]
    if let Ok(connector) = installed_connector() {
        if connector.join("Contents/PlugIns/TDM Safari Connector Extension.appex").exists() {
            std::thread::spawn(move || { let _ = register_single_copy(&connector); });
        }
    }
}

#[cfg(target_os = "macos")]
fn install_connector(source: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let destination = installed_connector()?;
    let version = bundle_value(source, "CFBundleShortVersionString").ok_or("Safari 安装器版本无效")?;
    if destination.exists() && bundle_value(&destination, "CFBundleIdentifier").as_deref() != Some("com.tdm.downloader.safari") {
        return Err("用户 Applications 目录存在同名的其他应用，请先移动该应用后重试。".into());
    }
    let source_team = signing_team(source);
    let installed_team = signing_team(&destination);
    let signed_downgrade = source_team.is_none() && installed_team.is_some();
    let needs_update = crate::extension_manager::needs_update(
        bundle_value(&destination, "CFBundleShortVersionString").as_deref(), &version);
    let signature_upgrade = source_team.is_some() && source_team != installed_team;
    if !signed_downgrade && (needs_update || signature_upgrade) {
        let parent = destination.parent().ok_or("无效的应用目录")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let stage = parent.join(".TDM Safari Connector.installing.app");
        let backup = parent.join(".TDM Safari Connector.previous.app");
        for path in [&stage, &backup] { if path.exists() { std::fs::remove_dir_all(path).map_err(|e| e.to_string())?; } }
        let output = std::process::Command::new("/usr/bin/ditto").arg(source).arg(&stage).output().map_err(|e| e.to_string())?;
        if !output.status.success() { return Err("复制 Safari 安装器失败，请检查磁盘空间。".into()); }
        let signature = std::process::Command::new("/usr/bin/codesign").args(["--verify", "--deep", "--strict"])
            .arg(&stage).output().map_err(|e| e.to_string())?;
        if !signature.status.success() { return Err("Safari 安装器签名完整性检查失败。".into()); }
        if destination.exists() { std::fs::rename(&destination, &backup).map_err(|e| e.to_string())?; }
        if let Err(error) = std::fs::rename(&stage, &destination) {
            if backup.exists() { let _ = std::fs::rename(&backup, &destination); }
            return Err(error.to_string());
        }
        if backup.exists() { let _ = std::fs::remove_dir_all(backup); }
        // A signing identity change also changes Safari's extension identity.
        let _ = std::process::Command::new("/usr/bin/pluginkit").arg("-r")
            .arg(destination.join("Contents/PlugIns/TDM Safari Connector Extension.appex")).output();
    }
    register_single_copy(&destination)?;
    Ok(destination)
}

/// Install/open a single stable container rather than registering every build path.
#[tauri::command]
pub async fn open_safari_connector(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut candidates = Vec::new();
        if let Ok(resources) = app.path().resource_dir() {
            candidates.push(resources.join("safari/TDM Safari Connector.app"));
        }
        #[cfg(debug_assertions)]
        candidates.push(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../release/safari/TDM Safari Connector.app"));
        let source = candidates.into_iter().find(|path| path.join("Contents/MacOS/TDM Safari Connector").is_file())
            .ok_or("未找到 Safari 扩展安装器，请使用包含 Safari 扩展的新版 macOS 安装包。")?;
        tauri::async_runtime::spawn_blocking(move || {
            let connector = install_connector(&source)?;
            let output = std::process::Command::new("/usr/bin/open").arg(connector).output().map_err(|e| e.to_string())?;
            if !output.status.success() { return Err(format!("无法打开 Safari 安装器：{}", String::from_utf8_lossy(&output.stderr))); }
            Ok(())
        }).await.map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "macos"))]
    { let _ = app; Err("Safari 扩展仅支持 macOS。".into()) }
}
