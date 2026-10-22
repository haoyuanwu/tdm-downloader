//! 状态栏弹窗定位。macOS 全程使用 AppKit 的全局逻辑点坐标。

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, Default)]
struct NativeAnchor {
    icon: Rect,
    bounds: Rect,
}

#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    pub bounds: Rect,
    icon: Rect,
    scale: f64,
}

impl Anchor {
    fn placement(self, height: f64) -> Rect {
        let margin = 8.0 * self.scale;
        let width = (380.0 * self.scale).min((self.bounds.width - 2.0 * margin).max(1.0));
        let height = (height * self.scale).min((self.bounds.height - 2.0 * margin).max(1.0));
        let left = self.bounds.x + margin;
        let top = self.bounds.y + margin;
        let below = self.icon.y + self.icon.height + 6.0 * self.scale;
        // Windows 托盘通常在底部；下方放不下时向上展开，避免遮挡任务栏。
        let y = if below + height > self.bounds.y + self.bounds.height - margin {
            self.icon.y - 6.0 * self.scale - height
        } else {
            below
        };
        Rect {
            x: (self.icon.x + self.icon.width / 2.0 - width / 2.0).clamp(
                left,
                (self.bounds.x + self.bounds.width - margin - width).max(left),
            ),
            y: y.clamp(
                top,
                (self.bounds.y + self.bounds.height - margin - height).max(top),
            ),
            width,
            height,
        }
    }
}

/// 在点击事件中立即解析锚点，保留当前点击的屏幕和图标矩形。
pub fn resolve(app: &tauri::AppHandle, clicked: Option<tauri::Rect>) -> Option<Anchor> {
    let rect = clicked.or_else(|| app.tray_by_id("main")?.rect().ok().flatten());
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn tdm_popup_anchor(
                has_rect: i32,
                x: f64,
                y: f64,
                width: f64,
                height: f64,
                result: *mut NativeAnchor,
            ) -> i32;
        }
        let mut native = NativeAnchor::default();
        // tray-icon 在 macOS 返回 Physical 矩形，原样传递，交给 AppKit 按目标屏幕解码。
        let (position, size) = rect
            .as_ref()
            .map(|rect| {
                (
                    rect.position.to_physical::<f64>(1.0),
                    rect.size.to_physical::<f64>(1.0),
                )
            })
            .unwrap_or_default();
        let ok = unsafe {
            tdm_popup_anchor(
                i32::from(rect.is_some()),
                position.x,
                position.y,
                size.width,
                size.height,
                &mut native,
            )
        };
        (ok != 0).then_some(Anchor {
            bounds: native.bounds,
            icon: native.icon,
            scale: 1.0,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let position = app.cursor_position().ok()?;
        // 自动唤起时鼠标可能位于另一块屏幕，应优先使用托盘所在屏幕。
        let point = rect
            .as_ref()
            .map(|rect| {
                let p = rect.position.to_physical::<f64>(1.0);
                let s = rect.size.to_physical::<f64>(1.0);
                (p.x + s.width / 2.0, p.y + s.height / 2.0)
            })
            .unwrap_or((position.x, position.y));
        let monitor = app.monitor_from_point(point.0, point.1).ok()??;
        let scale = monitor.scale_factor();
        let work_area = monitor.work_area();
        let origin = &work_area.position;
        let size = &work_area.size;
        let icon = rect
            .map(|rect| {
                let p = rect.position.to_physical::<f64>(scale);
                let s = rect.size.to_physical::<f64>(scale);
                Rect {
                    x: p.x,
                    y: p.y,
                    width: s.width,
                    height: s.height,
                }
            })
            .unwrap_or(Rect {
                x: position.x,
                y: position.y,
                width: 0.0,
                height: 0.0,
            });
        Some(Anchor {
            bounds: Rect {
                x: origin.x as f64,
                y: origin.y as f64,
                width: size.width as f64,
                height: size.height as f64,
            },
            icon,
            scale,
        })
    }
}

pub fn apply(window: &tauri::WebviewWindow, anchor: Anchor, height: f64) {
    let rect = anchor.placement(height);
    #[cfg(target_os = "macos")]
    {
        let _ = window.set_size(tauri::LogicalSize::new(rect.width, rect.height));
        let _ = window.set_position(tauri::LogicalPosition::new(rect.x, rect.y));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window.set_size(tauri::PhysicalSize::new(
            rect.width as u32,
            rect.height as u32,
        ));
        let _ = window.set_position(tauri::PhysicalPosition::new(rect.x as i32, rect.y as i32));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(x: f64, y: f64, width: f64, height: f64, icon_x: f64) -> Anchor {
        Anchor {
            bounds: Rect {
                x,
                y,
                width,
                height,
            },
            icon: Rect {
                x: icon_x,
                y: y - 24.0,
                width: 24.0,
                height: 24.0,
            },
            scale: 1.0,
        }
    }

    #[test]
    fn popup_stays_on_left_right_and_upper_displays() {
        for a in [
            anchor(-1920.0, 24.0, 1920.0, 1056.0, -100.0),
            anchor(1512.0, 324.0, 2560.0, 1416.0, 3700.0),
            anchor(0.0, -1056.0, 1920.0, 1056.0, 1400.0),
        ] {
            for height in [174.0, 390.0] {
                let p = a.placement(height);
                assert!(p.x >= a.bounds.x + 8.0);
                assert!(p.x + p.width <= a.bounds.x + a.bounds.width - 8.0);
                assert!(p.y >= a.bounds.y + 8.0);
                assert!(p.y + p.height <= a.bounds.y + a.bounds.height - 8.0);
            }
        }
    }

    #[test]
    fn popup_centers_on_clicked_icon_and_clamps_both_edges() {
        assert_eq!(
            anchor(0.0, 24.0, 1920.0, 1056.0, 900.0).placement(174.0).x,
            722.0
        );
        assert_eq!(
            anchor(0.0, 24.0, 1920.0, 1056.0, 0.0).placement(174.0).x,
            8.0
        );
        assert_eq!(
            anchor(0.0, 24.0, 1920.0, 1056.0, 1900.0).placement(174.0).x,
            1532.0
        );
    }

    #[test]
    fn bottom_taskbar_popup_opens_above_icon_at_mixed_dpi() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let a = Anchor {
                bounds: Rect {
                    x: -2560.0,
                    y: 0.0,
                    width: 2560.0,
                    height: 1400.0,
                },
                icon: Rect {
                    x: -80.0,
                    y: 1410.0,
                    width: 24.0 * scale,
                    height: 24.0 * scale,
                },
                scale,
            };
            let p = a.placement(390.0);
            assert!(p.y + p.height < a.icon.y);
            assert!(p.y + p.height <= a.bounds.height - 8.0 * scale);
            assert!(p.x + p.width <= -8.0 * scale);
            assert_eq!(p.width, 380.0 * scale);
        }
    }
}
