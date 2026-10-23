use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::Manager;

#[derive(Serialize)]
pub struct ExtensionStatus {
    chrome_version: String,
    chrome_installed_version: Option<String>,
    bridge_version: String,
    bridge_installed_version: Option<String>,
    safari_version: String,
    chrome_extension_id: Option<String>,
    chrome_directory: String,
}

fn read_version(path: &Path) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).ok()?).ok()?
        .get("version")?.as_str().map(str::to_owned)
}

pub(crate) fn needs_update(installed: Option<&str>, available: &str) -> bool {
    let Some(installed) = installed else { return true; };
    let parse = |version: &str| version.split('.').map(str::parse::<u32>).collect::<Result<Vec<_>, _>>();
    match (parse(installed), parse(available)) {
        (Ok(installed), Ok(available)) => available > installed,
        _ => false,
    }
}

#[cfg(not(windows))]
fn home() -> Result<PathBuf, String> {
    std::env::var_os("HOME").map(PathBuf::from).ok_or("无法读取用户目录".into())
}

fn chrome_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    return std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
        .map(|path| path.join("TDM Browser Extensions/chrome")).ok_or("无法读取本地应用目录".into());
    #[cfg(not(windows))]
    Ok(home()?.join("Library/Application Support/TDM Browser Extensions/chrome"))
}

fn bridge_dir() -> Result<PathBuf, String> {
    super::browser_paths::data_dir()
}

fn resource(app: &tauri::AppHandle, relative: &str) -> Result<PathBuf, String> {
    if let Ok(root) = app.path().resource_dir() {
        let path = root.join(relative);
        if path.exists() { return Ok(path); }
    }
    #[cfg(debug_assertions)]
    {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let path = match relative {
            "browser/chrome" => root.join("browser-extension/chrome"),
            "browser/native-version.json" => root.join(if cfg!(windows) { "browser-extension/native/windows-version.json" } else { "browser-extension/native/version.json" }),
            "browser/tdm-browser-bridge" => root.join("release/browser-native/tdm-browser-bridge"),
            "browser/tdm-browser-bridge.exe" => root.join("release/browser-native-windows/tdm-browser-bridge.exe"),
            _ => return Err("未知扩展资源".into()),
        };
        if path.exists() { return Ok(path); }
    }
    Err("此安装包未包含扩展管理资源，请使用包含此功能的新版 TDM。".into())
}

fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|letter| (b'a'..=b'p').contains(&letter))
}

fn registered_id() -> Option<String> {
    #[cfg(windows)]
    let path = bridge_dir().ok()?.join("com.tdm.downloader.bridge.json");
    #[cfg(not(windows))]
    let path = home().ok()?.join("Library/Application Support/Google/Chrome/NativeMessagingHosts/com.tdm.downloader.bridge.json");
    let manifest: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let id = manifest["allowed_origins"].as_array()?.last()?.as_str()?.strip_prefix("chrome-extension://")?.strip_suffix('/')?;
    valid_id(id).then(|| id.to_owned())
}

#[cfg(not(windows))]
fn standalone_app() -> Result<PathBuf, String> {
    #[cfg(not(debug_assertions))]
    if let Some(path) = std::env::current_exe().ok().and_then(|exe| exe.ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app")).map(Path::to_path_buf)) {
        return Ok(path);
    }
    #[cfg(debug_assertions)]
    {
        let preview = Path::new(env!("CARGO_MANIFEST_DIR")).join("../release/TDM Downloader Preview.app");
        if preview.exists() { return Ok(preview.canonicalize().map_err(|e| e.to_string())?); }
    }
    let installed = PathBuf::from("/Applications/TDM Downloader.app");
    if installed.exists() { return Ok(installed); }
    Err("请先安装可独立运行的 TDM 正式版，浏览器连接器不应依赖开发服务器。".into())
}

#[cfg(windows)]
fn standalone_app() -> Result<PathBuf, String> {
    if cfg!(debug_assertions) {
        return Err("请使用正式构建的 Windows 程序配置连接器，避免浏览器依赖开发服务器。".into());
    }
    std::env::current_exe().map_err(|e| e.to_string())
}

fn bridge_name() -> &'static str {
    if cfg!(windows) { "tdm-browser-bridge.exe" } else { "tdm-browser-bridge" }
}

/// Refresh only the app path on app launch; extension files and IDs stay unchanged.
pub fn refresh_app_path() {
    #[cfg(all(any(windows, target_os = "macos"), not(debug_assertions)))]
    if let (Ok(directory), Ok(app_path)) = (bridge_dir(), standalone_app()) {
        if directory.join("app-path.txt").exists() {
            let _ = std::fs::write(directory.join("app-path.txt"), app_path.to_string_lossy().as_bytes());
            #[cfg(windows)]
            if directory.join("com.tdm.downloader.bridge.json").exists() {
                let _ = register_windows_host(&directory.join("com.tdm.downloader.bridge.json"), &app_path);
            }
        }
    }
}

#[tauri::command]
pub fn browser_extension_status(app: tauri::AppHandle) -> Result<ExtensionStatus, String> {
    let chrome = resource(&app, "browser/chrome")?;
    let bridge = resource(&app, "browser/native-version.json")?;
    let directory = chrome_dir()?;
    Ok(ExtensionStatus {
        chrome_version: read_version(&chrome.join("manifest.json")).ok_or("Chrome 扩展版本无效")?,
        chrome_installed_version: read_version(&directory.join("manifest.json")),
        bridge_version: read_version(&bridge).ok_or("连接器版本无效")?,
        bridge_installed_version: read_version(&bridge_dir()?.join("version.json")),
        safari_version: serde_json::from_str::<serde_json::Value>(include_str!("../../browser-extension/safari/manifest.json"))
            .map_err(|e| e.to_string())?["version"].as_str().unwrap_or("未知").into(),
        chrome_extension_id: registered_id(),
        chrome_directory: directory.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn prepare_chrome_extension(app: tauri::AppHandle) -> Result<ExtensionStatus, String> {
    if !cfg!(any(windows, target_os = "macos")) { return Err("当前平台不支持一键配置。".into()); }
    let status = browser_extension_status(app.clone())?;
    let chrome = resource(&app, "browser/chrome")?;
    let destination = chrome_dir()?;
    let bridge = bridge_dir()?;
    let app_path = standalone_app()?;
    std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&bridge).map_err(|e| e.to_string())?;
    if needs_update(status.chrome_installed_version.as_deref(), &status.chrome_version) {
        for name in ["background.js", "popup.js", "popup.html", "popup.css"] {
            std::fs::copy(chrome.join(name), destination.join(name)).map_err(|e| e.to_string())?;
        }
        // Publish version only after all extension files are present.
        std::fs::copy(chrome.join("manifest.json"), destination.join("manifest.json")).map_err(|e| e.to_string())?;
    }
    if needs_update(status.bridge_installed_version.as_deref(), &status.bridge_version) || !bridge.join(bridge_name()).is_file() {
        let pending = bridge.join("tdm-browser-bridge.update");
        let source = resource(&app, &format!("browser/{}", bridge_name()))?;
        std::fs::copy(source, &pending).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&pending, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        }
        let destination = bridge.join(bridge_name());
        // rename 在 Windows 使用替换语义；若 EXE 仍在使用中，旧文件与版本号保持不变。
        std::fs::rename(pending, destination).map_err(|e| format!("更新连接器失败，请完成浏览器确认后重试：{e}"))?;
        std::fs::copy(resource(&app, "browser/native-version.json")?, bridge.join("version.json")).map_err(|e| e.to_string())?;
    }
    std::fs::write(bridge.join("app-path.txt"), app_path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    browser_extension_status(app)
}

#[tauri::command]
pub fn configure_chrome_extension(app: tauri::AppHandle, extension_id: String) -> Result<ExtensionStatus, String> {
    let id = extension_id.trim();
    if !valid_id(id) { return Err("扩展 ID 必须是 32 位 a-p 小写字母，请从 Chrome 扩展管理页复制。".into()); }
    prepare_chrome_extension(app.clone())?;
    #[cfg(windows)]
    let directory = bridge_dir()?;
    #[cfg(not(windows))]
    let directory = home()?.join("Library/Application Support/Google/Chrome/NativeMessagingHosts");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let mut origins = Vec::<String>::new();
    #[cfg(windows)]
    if let Ok(bytes) = std::fs::read(directory.join("com.tdm.downloader.bridge.json")) {
        if let Ok(previous) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(allowed) = previous["allowed_origins"].as_array() {
                for origin in allowed.iter().filter_map(|value| value.as_str()) {
                    if origin.strip_prefix("chrome-extension://").and_then(|id| id.strip_suffix('/')).is_some_and(valid_id) {
                        origins.push(origin.to_owned());
                    }
                }
            }
        }
    }
    let origin = format!("chrome-extension://{id}/");
    origins.retain(|value| value != &origin);
    origins.push(origin);
    let manifest = serde_json::json!({
        "name": "com.tdm.downloader.bridge", "description": "TDM browser download connector",
        "path": bridge_dir()?.join(bridge_name()), "type": "stdio",
        "allowed_origins": origins
    });
    std::fs::write(directory.join("com.tdm.downloader.bridge.json"), serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    register_windows_host(&directory.join("com.tdm.downloader.bridge.json"), &standalone_app()?)?;
    browser_extension_status(app)
}

#[cfg(windows)]
fn register_windows_host(manifest: &Path, executable: &Path) -> Result<(), String> {
    use winreg::{RegKey, enums::HKEY_CURRENT_USER};
    let user = RegKey::predef(HKEY_CURRENT_USER);
    for browser in ["Google\\Chrome", "Microsoft\\Edge"] {
        let (key, _) = user.create_subkey(format!("Software\\{browser}\\NativeMessagingHosts\\com.tdm.downloader.bridge"))
            .map_err(|e| e.to_string())?;
        key.set_value("", &manifest.to_string_lossy().as_ref()).map_err(|e| e.to_string())?;
    }
    let (scheme, _) = user.create_subkey("Software\\Classes\\tdm").map_err(|e| e.to_string())?;
    scheme.set_value("", &"URL:TDM Downloader").map_err(|e| e.to_string())?;
    scheme.set_value("URL Protocol", &"").map_err(|e| e.to_string())?;
    let (command, _) = scheme.create_subkey("shell\\open\\command").map_err(|e| e.to_string())?;
    command.set_value("", &format!("\"{}\" \"%1\"", executable.display())).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_chrome_extension_directory(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let directory = chrome_dir()?;
    if !directory.join("manifest.json").exists() { return Err("请先点击准备插件。".into()); }
    app.opener().open_path(directory.to_string_lossy().into_owned(), None::<&str>).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_updates_do_not_reinstall_equal_or_downgrade_newer_extensions() {
        assert!(super::needs_update(None, "0.1.1"));
        assert!(super::needs_update(Some("0.1.1"), "0.1.2"));
        assert!(!super::needs_update(Some("0.1.1"), "0.1.1"));
        assert!(!super::needs_update(Some("0.2.0"), "0.1.2"));
    }
    #[test]
    fn extension_identity_cannot_inject_an_origin_or_path() {
        assert!(super::valid_id("nnlbgoddcjacgbakbaodjkfgdhlpojnj"));
        for id in ["", "../host", "https://example.com", "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz", "NNLBGODDCJACGBAKBAODJKFGDHLPOJNJ"] {
            assert!(!super::valid_id(id));
        }
    }
}
