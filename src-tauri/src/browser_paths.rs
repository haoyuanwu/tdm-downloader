use std::path::PathBuf;

pub fn data_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    return std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("TDM Browser Bridge"))
        .ok_or_else(|| "无法读取本地应用目录".into());
    #[cfg(not(windows))]
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Application Support/TDM Browser Bridge"))
        .ok_or_else(|| "无法读取用户目录".into())
}

pub fn decision_dir() -> Result<PathBuf, String> {
    #[cfg(windows)]
    return data_dir().map(|path| path.join("decisions"));
    #[cfg(not(windows))]
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("Library/Caches/com.tdm.downloader.bridge"))
        .ok_or_else(|| "无法读取用户目录".into())
}
