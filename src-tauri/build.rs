fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        cc::Build::new()
            .file("native/notifications.m")
            .file("native/popup_position.m")
            .flag("-fobjc-arc")
            .compile("tdm_notifications");
        println!("cargo:rustc-link-lib=framework=UserNotifications");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rerun-if-changed=native/notifications.m");
        println!("cargo:rerun-if-changed=native/popup_position.m");
    }
    tauri_build::build()
}
