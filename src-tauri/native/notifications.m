#import <Foundation/Foundation.h>
#import <UserNotifications/UserNotifications.h>

typedef void (*TDMNotificationCallback)(void *context, const char *json);
typedef void (*TDMNotificationClickHandler)(void);
static TDMNotificationClickHandler notificationClickHandler;

void tdm_notification_set_click_handler(TDMNotificationClickHandler handler) {
    notificationClickHandler = handler;
}

static void TDMReply(TDMNotificationCallback callback, void *context, NSDictionary *reply) {
    @autoreleasepool {
        NSData *data = [NSJSONSerialization dataWithJSONObject:reply options:0 error:nil];
        NSString *json = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
        callback(context, json.UTF8String);
    }
}

static void TDMError(TDMNotificationCallback callback, void *context, NSString *message) {
    TDMReply(callback, context, @{ @"ok": @NO, @"error": message });
}

@interface TDMNotificationDelegate : NSObject <UNUserNotificationCenterDelegate>
@end

@implementation TDMNotificationDelegate
- (void)userNotificationCenter:(UNUserNotificationCenter *)center
      willPresentNotification:(UNNotification *)notification
        withCompletionHandler:(void (^)(UNNotificationPresentationOptions))completionHandler {
    // 前台也显示横幅，并保留在通知中心。声音由应用独立播放，避免重复响铃。
    completionHandler(UNNotificationPresentationOptionBanner | UNNotificationPresentationOptionList);
}

- (void)userNotificationCenter:(UNUserNotificationCenter *)center
 didReceiveNotificationResponse:(UNNotificationResponse *)response
         withCompletionHandler:(void (^)(void))completionHandler {
    if ([response.actionIdentifier isEqualToString:UNNotificationDefaultActionIdentifier]
        && notificationClickHandler) {
        notificationClickHandler();
    }
    completionHandler();
}
@end

static UNUserNotificationCenter *TDMCenter(TDMNotificationCallback callback, void *context) {
    if (NSBundle.mainBundle.bundleIdentifier.length == 0) {
        TDMError(callback, context, @"应用缺少 macOS 身份，请使用 npm run tauri dev 启动开发版。");
        return nil;
    }
    static TDMNotificationDelegate *delegate;
    static dispatch_once_t once;
    dispatch_once(&once, ^{ delegate = [TDMNotificationDelegate new]; });
    UNUserNotificationCenter *center = UNUserNotificationCenter.currentNotificationCenter;
    center.delegate = delegate;
    return center;
}

void tdm_notification_permission(TDMNotificationCallback callback, void *context) {
    @autoreleasepool {
        UNUserNotificationCenter *center = TDMCenter(callback, context);
        if (!center) return;
        [center requestAuthorizationWithOptions:UNAuthorizationOptionAlert | UNAuthorizationOptionSound
                             completionHandler:^(BOOL granted, NSError *error) {
            if (error) {
                TDMError(callback, context, error.localizedDescription);
            } else {
                TDMReply(callback, context, @{ @"ok": @YES, @"granted": granted ? @YES : @NO });
            }
        }];
    }
}

void tdm_notification_status(TDMNotificationCallback callback, void *context) {
    @autoreleasepool {
        UNUserNotificationCenter *center = TDMCenter(callback, context);
        if (!center) return;
        NSString *bundleID = NSBundle.mainBundle.bundleIdentifier;
        [center getNotificationSettingsWithCompletionHandler:^(UNNotificationSettings *settings) {
            [center getDeliveredNotificationsWithCompletionHandler:^(NSArray<UNNotification *> *notifications) {
                NSMutableArray *ids = [NSMutableArray new];
                for (UNNotification *notification in notifications) {
                    [ids addObject:notification.request.identifier];
                }
                TDMReply(callback, context, @{
                    @"ok": @YES,
                    @"authorization": @(settings.authorizationStatus),
                    @"alerts_enabled": settings.alertSetting == UNNotificationSettingEnabled ? @YES : @NO,
                    @"center_enabled": settings.notificationCenterSetting == UNNotificationSettingEnabled ? @YES : @NO,
                    @"bundle_id": bundleID,
                    @"delivered_ids": ids
                });
            }];
        }];
    }
}

void tdm_notification_send(const char *identifier, const char *title, const char *body,
                           TDMNotificationCallback callback, void *context) {
    @autoreleasepool {
        UNUserNotificationCenter *center = TDMCenter(callback, context);
        if (!center) return;
        UNMutableNotificationContent *content = [UNMutableNotificationContent new];
        content.title = [NSString stringWithUTF8String:title];
        content.body = [NSString stringWithUTF8String:body];
        content.threadIdentifier = @"tdm.downloads";
        UNNotificationRequest *request = [UNNotificationRequest
            requestWithIdentifier:[NSString stringWithUTF8String:identifier] content:content trigger:nil];
        [center addNotificationRequest:request withCompletionHandler:^(NSError *error) {
            if (error) TDMError(callback, context, error.localizedDescription);
            else TDMReply(callback, context, @{ @"ok": @YES });
        }];
    }
}
