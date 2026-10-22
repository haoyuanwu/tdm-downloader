//! macOS 原生通知：真实授权、前台展示和可查询的送达记录。

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::Manager;

#[derive(Default)]
pub struct NotificationState(Mutex<Option<String>>);

#[derive(Serialize, Default)]
pub struct NotificationStatus {
    authorization: String,
    alerts_enabled: bool,
    center_enabled: bool,
    bundle_id: String,
    delivered_ids: Vec<String>,
    last_error: Option<String>,
}

fn record_result(app: &tauri::AppHandle, result: &Result<(), String>) {
    if let Ok(mut error) = app.state::<NotificationState>().0.lock() {
        *error = result.as_ref().err().cloned();
    }
}

pub async fn initialize(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    native::register_click_handler(app.clone());
    let result = request_permission().await;
    if let Err(error) = &result {
        eprintln!("通知授权：{error}");
    }
    record_result(&app, &result);
}

pub async fn completed(
    app: &tauri::AppHandle,
    id: &str,
    title: &str,
    body: &str,
) -> Result<(), String> {
    // 声音独立于通知授权，且收尾动作需等声音播完和通知提交后才执行。
    let (visual, audio) = tokio::join!(send_visual(app, id, title, body), play_sound());
    let result = visual.and(audio);
    record_result(app, &result);
    result
}

#[cfg(target_os = "macos")]
async fn play_sound() -> Result<(), String> {
    let status = tokio::process::Command::new("/usr/bin/afplay")
        .arg("/System/Library/Sounds/Glass.aiff")
        .status()
        .await
        .map_err(|error| format!("播放提示音失败：{error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("提示音播放失败：{status}"))
    }
}

#[cfg(not(target_os = "macos"))]
async fn play_sound() -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use std::ffi::{c_char, c_void, CStr, CString};
    use tokio::sync::oneshot;

    type Callback = unsafe extern "C" fn(*mut c_void, *const c_char);
    static CLICK_APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();
    unsafe extern "C" {
        fn tdm_notification_set_click_handler(handler: unsafe extern "C" fn());
        fn tdm_notification_permission(callback: Callback, context: *mut c_void);
        fn tdm_notification_status(callback: Callback, context: *mut c_void);
        fn tdm_notification_send(
            id: *const c_char,
            title: *const c_char,
            body: *const c_char,
            callback: Callback,
            context: *mut c_void,
        );
    }

    unsafe extern "C" fn clicked() {
        if let Some(app) = CLICK_APP.get() {
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                use tauri::Emitter;
                let _ = handle.emit("tdm://show-downloads", ());
                if let Err(error) = crate::show_main_window(handle) {
                    eprintln!("点击通知打开主窗口失败：{error}");
                }
            });
        }
    }

    pub fn register_click_handler(app: tauri::AppHandle) {
        let _ = CLICK_APP.set(app);
        unsafe { tdm_notification_set_click_handler(clicked) };
    }

    #[derive(Deserialize, Default)]
    pub struct Reply {
        ok: bool,
        error: Option<String>,
        pub granted: Option<bool>,
        #[serde(default)]
        pub authorization: u32,
        #[serde(default)]
        pub alerts_enabled: bool,
        #[serde(default)]
        pub center_enabled: bool,
        #[serde(default)]
        pub bundle_id: String,
        #[serde(default)]
        pub delivered_ids: Vec<String>,
    }

    // Cocoa 每个请求恰好回调一次；回调复制 JSON 并归还该请求的 sender。
    unsafe extern "C" fn callback(context: *mut c_void, json: *const c_char) {
        let sender = unsafe { Box::from_raw(context.cast::<oneshot::Sender<String>>()) };
        let value = if json.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(json) }
                .to_string_lossy()
                .into_owned()
        };
        let _ = sender.send(value);
    }

    async fn receive(receiver: oneshot::Receiver<String>) -> Result<Reply, String> {
        let json = receiver
            .await
            .map_err(|error| format!("通知回调中断：{error}"))?;
        let reply: Reply =
            serde_json::from_str(&json).map_err(|error| format!("通知响应无效：{error}"))?;
        if reply.ok {
            Ok(reply)
        } else {
            Err(reply.error.unwrap_or_else(|| "macOS 拒绝了通知请求".into()))
        }
    }

    pub async fn permission() -> Result<(), String> {
        let (sender, receiver) = oneshot::channel();
        unsafe { tdm_notification_permission(callback, Box::into_raw(Box::new(sender)).cast()) };
        if receive(receiver).await?.granted == Some(true) {
            Ok(())
        } else {
            Err("通知权限已关闭，请在系统设置 → 通知中允许 TDM Downloader 发送通知".into())
        }
    }

    pub async fn status() -> Result<Reply, String> {
        let (sender, receiver) = oneshot::channel();
        unsafe { tdm_notification_status(callback, Box::into_raw(Box::new(sender)).cast()) };
        receive(receiver).await
    }

    pub async fn send(id: &str, title: &str, body: &str) -> Result<(), String> {
        permission().await?;
        let clean = |value: &str| CString::new(value.replace('\0', "")).expect("NUL removed");
        let (id, title, body) = (clean(id), clean(title), clean(body));
        let (sender, receiver) = oneshot::channel();
        unsafe {
            tdm_notification_send(
                id.as_ptr(),
                title.as_ptr(),
                body.as_ptr(),
                callback,
                Box::into_raw(Box::new(sender)).cast(),
            )
        };
        receive(receiver).await.map(|_| ())
    }
}

async fn request_permission() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return native::permission().await;
    #[cfg(not(target_os = "macos"))]
    Ok(())
}

async fn send_visual(
    app: &tauri::AppHandle,
    id: &str,
    title: &str,
    body: &str,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        native::send(id, title, body).await
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri_plugin_notification::NotificationExt;
        let _ = id;
        app.notification()
            .builder()
            .title(title)
            .body(body)
            .sound("Default")
            .show()
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
pub async fn notification_status(app: tauri::AppHandle) -> Result<NotificationStatus, String> {
    #[cfg(target_os = "macos")]
    let status = {
        let reply = native::status().await?;
        NotificationStatus {
            authorization: match reply.authorization {
                0 => "not_determined",
                1 => "denied",
                2..=4 => "authorized",
                _ => "unknown",
            }
            .into(),
            alerts_enabled: reply.alerts_enabled,
            center_enabled: reply.center_enabled,
            bundle_id: reply.bundle_id,
            delivered_ids: reply.delivered_ids,
            ..Default::default()
        }
    };
    #[cfg(not(target_os = "macos"))]
    let status = NotificationStatus {
        authorization: "authorized".into(),
        alerts_enabled: true,
        center_enabled: true,
        ..Default::default()
    };
    let last_error = app
        .state::<NotificationState>()
        .0
        .lock()
        .ok()
        .and_then(|error| error.clone());
    Ok(NotificationStatus {
        last_error,
        ..status
    })
}

#[tauri::command]
pub async fn test_download_notification(app: tauri::AppHandle) -> Result<String, String> {
    let id = format!("tdm.test.{}", uuid::Uuid::new_v4());
    completed(
        &app,
        &id,
        "TDM 通知测试",
        "下载完成时，会在这里显示文件名称，并播放提示音。",
    )
    .await?;
    #[cfg(target_os = "macos")]
    {
        let status = notification_status(app).await?;
        if !status.delivered_ids.contains(&id) {
            return Err(
                "提示音已播放，macOS 已接受通知，但通知中心尚无送达记录，请检查系统通知设置".into(),
            );
        }
    }
    Ok("测试通知已送达通知中心，提示音已播放".into())
}

#[tauri::command]
pub async fn open_notification_settings(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = notification_status(app).await?;
        let status = tokio::process::Command::new("/usr/bin/open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id={}",
                status.bundle_id
            ))
            .status()
            .await
            .map_err(|error| error.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("无法打开通知设置".into())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("请在系统设置中管理通知权限".into())
    }
}
