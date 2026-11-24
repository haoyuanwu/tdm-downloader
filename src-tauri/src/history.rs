//! Shared Windows/macOS download history with a recoverable last-good snapshot.
use crate::models::{DownloadTask, TaskStatus};
use serde::Deserialize;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
struct History {
    version: u32,
    tasks: Vec<DownloadTask>,
}

pub(crate) struct HistoryStore {
    path: PathBuf,
    last_saved: Vec<u8>,
    load_error: Option<String>,
}

impl HistoryStore {
    pub fn open(path: PathBuf) -> (Self, Vec<DownloadTask>) {
        let mut store = Self {
            path,
            last_saved: Vec::new(),
            load_error: None,
        };
        let read = |path: &Path| -> Result<Vec<DownloadTask>, String> {
            let data = fs::read(path).map_err(|error| error.to_string())?;
            if let Ok(tasks) = serde_json::from_slice::<Vec<DownloadTask>>(&data) {
                return Ok(tasks);
            }
            let history: History =
                serde_json::from_slice(&data).map_err(|error| error.to_string())?;
            if history.version != 1 {
                return Err("下载记录版本不受支持".into());
            }
            Ok(history.tasks)
        };
        let backup = store.path.with_extension("json.bak");
        let legacy = store.path.with_file_name("history.json");
        let source = if store.path.exists() || backup.exists() {
            &store.path
        } else {
            &legacy
        };
        let backup = source.with_extension("json.bak");
        let tasks = if !source.exists() && !backup.exists() {
            Vec::new()
        } else {
            match read(source) {
                Ok(tasks) => tasks,
                Err(primary) => match read(&backup) {
                    Ok(tasks) => {
                        // Keep the damaged primary for diagnosis; preserve the good backup.
                        if source.exists() {
                            if let Err(error) = fs::rename(
                                source,
                                source.with_extension(format!("corrupt-{}", uuid::Uuid::new_v4())),
                            ) {
                                store.load_error = Some(format!("无法保留损坏的下载记录：{error}"));
                            }
                        }
                        tasks
                    }
                    Err(_) => {
                        store.load_error =
                            Some(format!("读取下载记录失败，原文件已保留：{primary}"));
                        Vec::new()
                    }
                },
            }
        };
        if let Some(error) = &store.load_error {
            eprintln!("{error}");
        }
        (store, tasks.into_iter().map(restore_task).collect())
    }

    pub fn check_loaded(&self) -> Result<(), String> {
        self.load_error.as_ref().map_or(Ok(()), |error| Err(error.clone()))
    }

    pub fn save(&mut self, mut tasks: Vec<DownloadTask>) -> Result<(), String> {
        self.check_loaded()?;
        for task in &mut tasks {
            task.file_exists = None;
        }
        let data = serde_json::to_vec(&tasks).map_err(|error| error.to_string())?;
        if data == self.last_saved {
            return Ok(());
        }
        let backup = self.path.with_extension("json.bak");
        let result = (|| -> std::io::Result<()> {
            match fs::read(&self.path) {
                Ok(previous) => write_snapshot(&backup, &previous)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            write_snapshot(&self.path, &data)
        })();
        result.map_err(|error| format!("保存下载记录失败：{error}"))?;
        self.last_saved = data;
        Ok(())
    }
}

fn write_snapshot(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().ok_or_else(|| std::io::Error::other("任务存储路径无效"))?;
    fs::create_dir_all(dir)?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    temp.write_all(data)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    Ok(())
}

fn restore_task(mut task: DownloadTask) -> DownloadTask {
    task.file_exists = None;
    if task.status.is_active() || task.status == TaskStatus::Pending {
        // A crash can happen after merging removed the parts but before history saved.
        let merged = task.status == TaskStatus::Merging
            && task.total_size > 0
            && fs::metadata(&task.save_path).is_ok_and(|file| file.len() == task.total_size)
            && task
                .segments
                .iter()
                .all(|segment| !crate::engine::part_path(&task.save_path, segment.index).exists());
        task.status = if merged {
            TaskStatus::Completed
        } else {
            TaskStatus::Paused
        };
        if merged {
            task.downloaded = task.total_size;
        }
    }
    if !task.status.is_terminal() || task.status == TaskStatus::Failed {
        // Disk contents, rather than a possibly stale progress tick, define resume offsets.
        for segment in &mut task.segments {
            segment.downloaded =
                fs::metadata(crate::engine::part_path(&task.save_path, segment.index))
                    .map(|file| file.len())
                    .unwrap_or(0);
            if task.total_size > 0 {
                segment.downloaded = segment.downloaded.min(segment.total());
            }
            segment.done = task.total_size > 0 && segment.downloaded == segment.total();
        }
        task.downloaded = task.segments.iter().map(|segment| segment.downloaded).sum();
    }
    task
}
