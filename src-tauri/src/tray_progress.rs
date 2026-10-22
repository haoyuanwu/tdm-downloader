//! 菜单栏进度独立于窗口运行，隐藏主窗口后仍持续刷新。

use crate::{engine::DownloadEngine, models::TaskStatus};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tauri::Manager;

#[derive(Default)]
pub struct TrayProgressState {
    pub has_tasks: AtomicBool,
}

#[derive(Default)]
struct TaskRotation {
    selected: Option<String>,
    switched_at: Duration,
}

impl TaskRotation {
    fn select(&mut self, ids: &[&str], now: Duration) -> Option<usize> {
        if ids.is_empty() {
            self.selected = None;
            return None;
        }
        let previous = self
            .selected
            .as_ref()
            .and_then(|id| ids.iter().position(|v| v == id));
        let index = match previous {
            Some(index) if now.saturating_sub(self.switched_at) < Duration::from_secs(3) => {
                return Some(index)
            }
            Some(index) => (index + 1) % ids.len(),
            None => 0,
        };
        self.selected = Some(ids[index].to_string());
        self.switched_at = now;
        Some(index)
    }
}

pub async fn monitor(app: tauri::AppHandle) {
    let engine = app.state::<Arc<DownloadEngine>>().inner().clone();
    let original_icon = app.default_window_icon().cloned();
    let started = Instant::now();
    let mut rotation = TaskRotation::default();
    let mut showing_progress = false;
    let mut interval = tokio::time::interval(Duration::from_millis(500));
    loop {
        interval.tick().await;
        let tasks = engine.snapshot().await;
        app.state::<TrayProgressState>().has_tasks.store(
            tasks
                .iter()
                .any(|task| task.status != TaskStatus::Cancelled),
            Ordering::Relaxed,
        );
        let active: Vec<_> = tasks
            .iter()
            .filter(|task| task.status.is_active() || task.status == TaskStatus::Pending)
            .collect();
        let ids: Vec<_> = active.iter().map(|task| task.id.as_str()).collect();
        let selected = rotation.select(&ids, started.elapsed());
        let Some(tray) = app.tray_by_id("main") else {
            continue;
        };
        if let Some(index) = selected {
            let task = active[index];
            let progress = (task.total_size > 0 && task.total_size != u64::MAX)
                .then(|| (task.downloaded as f64 / task.total_size as f64).clamp(0.0, 1.0));
            let icon = progress_icon(
                active.len(),
                progress,
                started.elapsed().as_secs_f64() / 2.0,
            );
            let _ = tray.set_icon_with_as_template(Some(icon), true);
            let percent = progress
                .map(|p| format!("{:.0}%", p * 100.0))
                .unwrap_or_else(|| "大小未知".into());
            let tooltip = format!(
                "{} 个下载任务 · {} · {}",
                active.len(),
                task.filename,
                percent
            );
            let _ = tray.set_tooltip(Some(tooltip.as_str()));
            showing_progress = true;
        } else if showing_progress {
            let _ = tray.set_icon_with_as_template(original_icon.clone(), false);
            let _ = tray.set_tooltip(Some("TDM Downloader"));
            showing_progress = false;
        }
    }
}

// 以 2x 像素密度绘制模板图，系统负责在深浅色菜单栏中应用前景色。
// 数字用圆端矢量笔画绘制，避免依赖字体文件或后台窗口。
const DIGITS: [&[(f64, f64)]; 10] = [
    &[
        (1., 0.),
        (3., 0.),
        (4., 1.),
        (4., 6.),
        (3., 7.),
        (1., 7.),
        (0., 6.),
        (0., 1.),
        (1., 0.),
    ],
    &[(0.5, 1.5), (2., 0.), (2., 7.)],
    &[
        (0., 1.),
        (1., 0.),
        (3., 0.),
        (4., 1.),
        (4., 2.),
        (0., 6.),
        (0., 7.),
        (4., 7.),
    ],
    &[
        (0., 0.),
        (4., 0.),
        (2., 3.),
        (3., 3.),
        (4., 4.),
        (4., 6.),
        (3., 7.),
        (0., 7.),
    ],
    &[(3., 7.), (3., 0.), (0., 4.5), (4., 4.5)],
    &[
        (4., 0.),
        (0., 0.),
        (0., 3.),
        (3., 3.),
        (4., 4.),
        (4., 6.),
        (3., 7.),
        (0., 7.),
    ],
    &[
        (4., 0.),
        (1., 0.),
        (0., 2.),
        (0., 6.),
        (1., 7.),
        (3., 7.),
        (4., 6.),
        (4., 4.),
        (3., 3.),
        (0., 3.),
    ],
    &[(0., 0.), (4., 0.), (1., 7.)],
    &[
        (1., 3.5),
        (0., 2.5),
        (0., 1.),
        (1., 0.),
        (3., 0.),
        (4., 1.),
        (4., 2.5),
        (3., 3.5),
        (1., 3.5),
        (0., 4.5),
        (0., 6.),
        (1., 7.),
        (3., 7.),
        (4., 6.),
        (4., 4.5),
        (3., 3.5),
    ],
    &[
        (4., 4.),
        (1., 4.),
        (0., 3.),
        (0., 1.),
        (1., 0.),
        (3., 0.),
        (4., 1.),
        (4., 5.),
        (3., 7.),
        (0., 7.),
    ],
];

fn segment_distance(x: f64, y: f64, a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let t = (((x - a.0) * dx + (y - a.1) * dy) / (dx * dx + dy * dy)).clamp(0., 1.);
    (x - a.0 - t * dx).hypot(y - a.1 - t * dy)
}

pub(crate) fn progress_icon(
    count: usize,
    progress: Option<f64>,
    phase: f64,
) -> tauri::image::Image<'static> {
    const SIZE: u32 = 44;
    let label = if count > 99 {
        "99+".into()
    } else {
        count.to_string()
    };
    let width = label.len() as f64 * 6.0 - 2.0;
    let scale = (24.0 / width).min(2.8);
    let left = 22.0 - width * scale / 2.0;
    let top = 22.0 - 7.0 * scale / 2.0;
    let (start, length) = progress
        .map(|p| (0.0, p.clamp(0., 1.)))
        .unwrap_or((phase.fract(), 0.25));
    let tau = std::f64::consts::TAU;
    let radius = 18.5;
    let mut rgba = vec![0; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let px = x as f64 + 0.5;
            let py = y as f64 + 0.5;
            let dx = px - 22.;
            let dy = py - 22.;
            let ring_distance = (dx.hypot(dy) - radius).abs();
            let mut alpha = (1.3 - ring_distance).clamp(0., 1.) * 0.3;
            if length > 0.0 {
                let angle = (dy.atan2(dx) / tau + 0.25).rem_euclid(1.0);
                let in_arc = (angle - start).rem_euclid(1.0) <= length;
                let arc_distance = if in_arc {
                    ring_distance
                } else {
                    [start, start + length]
                        .into_iter()
                        .map(|turn| {
                            let theta = (turn - 0.25) * tau;
                            (dx - radius * theta.cos()).hypot(dy - radius * theta.sin())
                        })
                        .fold(f64::INFINITY, f64::min)
                };
                alpha = alpha.max((1.8 - arc_distance).clamp(0., 1.));
            }
            for (index, digit) in label.chars().enumerate() {
                let gx = (px - left) / scale - index as f64 * 6.;
                let gy = (py - top) / scale;
                let plus = [(0., 3.5), (4., 3.5), (2., 3.5), (2., 1.5), (2., 5.5)];
                let path = digit
                    .to_digit(10)
                    .map(|n| DIGITS[n as usize])
                    .unwrap_or(&plus);
                let distance = path
                    .windows(2)
                    .map(|line| segment_distance(gx, gy, line[0], line[1]))
                    .fold(f64::INFINITY, f64::min)
                    * scale;
                alpha = alpha.max((scale * 0.43 + 0.5 - distance).clamp(0., 1.));
            }
            let offset = ((y * SIZE + x) * 4) as usize;
            // macOS 模板图由系统着色；Windows 直接显示 RGBA，使用蓝色确保浅色任务栏可见。
            #[cfg(not(target_os = "macos"))]
            rgba[offset..offset + 3].copy_from_slice(&[36, 145, 235]);
            rgba[offset + 3] = (alpha * 255.).round() as u8;
        }
    }
    tauri::image::Image::new_owned(rgba, SIZE, SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_waits_three_seconds_and_preserves_task_when_queue_changes() {
        let mut rotation = TaskRotation::default();
        assert_eq!(rotation.select(&["a", "b"], Duration::ZERO), Some(0));
        assert_eq!(
            rotation.select(&["new", "a", "b"], Duration::from_millis(2999)),
            Some(1)
        );
        assert_eq!(
            rotation.select(&["new", "a", "b"], Duration::from_secs(3)),
            Some(2)
        );
        assert_eq!(
            rotation.select(&["new", "a", "b"], Duration::from_secs(6)),
            Some(0)
        );
        assert_eq!(rotation.select(&[], Duration::from_secs(7)), None);
        assert_eq!(rotation.select(&["b"], Duration::from_secs(8)), Some(0));
    }

    #[test]
    fn progress_arc_fills_clockwise_and_clamps_to_full_circle() {
        let quarter = progress_icon(1, Some(0.25), 0.);
        let full = progress_icon(1, Some(1.), 0.);
        let pixel_alpha =
            |icon: &tauri::image::Image<'_>, x: usize, y: usize| icon.rgba()[(y * 44 + x) * 4 + 3];
        assert!(pixel_alpha(&quarter, 40, 21) > 200);
        assert!(pixel_alpha(&quarter, 3, 21) < 100);
        assert!(pixel_alpha(&full, 3, 21) > 200);
        assert_eq!(full.rgba(), progress_icon(1, Some(2.), 0.).rgba());
    }
}
