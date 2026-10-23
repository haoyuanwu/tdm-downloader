import SafariServices
import AppKit

class SafariWebExtensionHandler: NSObject, NSExtensionRequestHandling {
    private func hostApplication() -> URL? {
        // The extension is nested inside its connector and, in distributed builds,
        // inside TDM itself. Prefer that exact app over Launch Services' old copies.
        var parent = Bundle.main.bundleURL.deletingLastPathComponent()
        while parent.path != "/" {
            if parent.pathExtension == "app",
               Bundle(url: parent)?.bundleIdentifier == "com.tdm.downloader" {
                return parent
            }
            parent.deleteLastPathComponent()
        }
        return NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.tdm.downloader")
    }

    private func complete(_ context: NSExtensionContext, _ response: [String: Any]) {
        let result = NSExtensionItem()
        result.userInfo = [SFExtensionMessageKey: response]
        context.completeRequest(returningItems: [result], completionHandler: nil)
    }

    func beginRequest(with context: NSExtensionContext) {
        let item = context.inputItems.first as? NSExtensionItem
        let message = item?.userInfo?[SFExtensionMessageKey] as? [String: Any]
        var response: [String: Any] = ["ok": false, "error": "无效的下载链接"]
        if let address = message?["url"] as? String, address.utf8.count < 24000,
           let target = URL(string: address), ["http", "https"].contains(target.scheme ?? ""),
           target.host != nil, target.user == nil, target.password == nil {
            var link = URLComponents()
            link.scheme = "tdm"; link.host = "download"
            link.queryItems = [URLQueryItem(name: "url", value: address), URLQueryItem(name: "filename", value: message?["filename"] as? String ?? "")]
            if let url = link.url, let host = hostApplication() {
                NSWorkspace.shared.open([url], withApplicationAt: host, configuration: NSWorkspace.OpenConfiguration()) { application, error in
                    let opened = application != nil && error == nil
                    self.complete(context, ["ok": opened, "error": opened ? "" : "无法打开 TDM，请先确认应用可以正常运行"])
                }
                return
            }
            response = ["ok": false, "error": "请先安装支持浏览器接管的 TDM 版本"]
        }
        complete(context, response)
    }
}
