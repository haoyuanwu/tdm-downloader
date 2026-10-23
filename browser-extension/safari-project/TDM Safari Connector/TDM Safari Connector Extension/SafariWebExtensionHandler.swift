import SafariServices
import AppKit

class SafariWebExtensionHandler: NSObject, NSExtensionRequestHandling {
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
            if let url = link.url {
                let opened = NSWorkspace.shared.open(url)
                response = ["ok": opened, "error": opened ? "" : "请先安装支持浏览器接管的 TDM 版本"]
            }
        }
        let result = NSExtensionItem()
        result.userInfo = [SFExtensionMessageKey: response]
        context.completeRequest(returningItems: [result], completionHandler: nil)
    }
}
