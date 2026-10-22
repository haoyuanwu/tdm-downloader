#import <AppKit/AppKit.h>
#import <CoreGraphics/CoreGraphics.h>

typedef struct {
    double x, y, width, height;
} TDMRect;

typedef struct {
    TDMRect icon;
    TDMRect bounds;
} TDMPopupAnchor;

// 必须在主线程调用。AppKit 的全局坐标使用点，避免不同屏幕的像素倍率混用。
int tdm_popup_anchor(int hasRect, double x, double y, double width, double height,
                     TDMPopupAnchor *result) {
    if (![NSThread isMainThread] || result == NULL) return 0;
    NSArray<NSScreen *> *screens = NSScreen.screens;
    NSScreen *primary = screens.firstObject;
    if (!primary) return 0;

    NSPoint mouse = NSEvent.mouseLocation;
    NSScreen *target = nil;
    for (NSScreen *screen in screens) {
        if (NSPointInRect(mouse, screen.frame)) {
            target = screen;
            break;
        }
    }
    if (!target) target = NSScreen.mainScreen ?: primary;

    double desktopTop = NSMaxY(primary.frame);
    NSRect frame = target.frame;
    NSRect visible = target.visibleFrame;
    double scale = target.backingScaleFactor;
    // tray-icon 的点击事件矩形按事件所在屏幕的倍率生成，不能用弹窗的旧倍率。
    // 与当前 tray-icon 的 flip_window_screen_coordinates 使用同一个主屏基准。
    double trayDesktopTop = CGDisplayPixelsHigh(CGMainDisplayID());
    NSRect icon = NSMakeRect(x / scale, trayDesktopTop - (y + height) / scale,
                             width / scale, height / scale);
    BOOL validIcon = hasRect && width > 0 && height > 0
        && NSPointInRect(NSMakePoint(NSMidX(icon), NSMidY(icon)), frame)
        && NSMaxY(icon) >= NSMaxY(visible) - 2;
    if (!validIcon) {
        // 剪贴板自动唤起或系统暂未提供图标矩形时，仍选择鼠标所在屏幕的菜单栏。
        double menuBottom = NSMaxY(visible);
        double menuHeight = MAX(1, NSMaxY(frame) - menuBottom);
        icon = NSMakeRect(mouse.x - 12, menuBottom, 24, menuHeight);
    }

    result->icon = (TDMRect){icon.origin.x, desktopTop - NSMaxY(icon),
                             icon.size.width, icon.size.height};
    result->bounds = (TDMRect){visible.origin.x, desktopTop - NSMaxY(visible),
                               visible.size.width, visible.size.height};
    return 1;
}
