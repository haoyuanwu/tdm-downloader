import Cocoa
import SafariServices
import WebKit

let extensionBundleIdentifier = "com.tdm.downloader.safari.Extension"

class ViewController: NSViewController, WKNavigationDelegate, WKScriptMessageHandler {
    @IBOutlet var webView: WKWebView!

    override func viewDidLoad() {
        super.viewDidLoad()
        webView.navigationDelegate = self
        webView.configuration.userContentController.add(self, name: "controller")
        webView.loadFileURL(Bundle.main.url(forResource: "Main", withExtension: "html")!, allowingReadAccessTo: Bundle.main.resourceURL!)
    }

    private func report(_ text: String) {
        // JSON encoding preserves Chinese and avoids interpolating executable script.
        guard let data = try? JSONSerialization.data(withJSONObject: [text]),
              let argument = String(data: data, encoding: .utf8) else { return }
        webView.evaluateJavaScript("showHelp(\(argument)[0])")
    }

    private func refreshState() {
        SFSafariExtensionManager.getStateOfSafariExtension(withIdentifier: extensionBundleIdentifier) { state, error in
            DispatchQueue.main.async {
                if let state = state, error == nil {
                    NSLog("TDM Safari state: recognized, enabled=%@", state.isEnabled ? "yes" : "no")
                    self.webView.evaluateJavaScript("show(\(state.isEnabled), true)")
                    self.report("")
                } else {
                    let failure = error as NSError?
                    NSLog("TDM Safari state: unavailable, domain=%@ code=%ld", failure?.domain ?? "unknown", failure?.code ?? -1)
                    self.webView.evaluateJavaScript("show(null, true)")
                    self.report("Safari 尚未识别此扩展。请先在 Safari → 设置 → 扩展中检查「TDM 下载助手」。临时签名版若未出现在列表中，需要手动在 Safari「开发」菜单中允许未签名的扩展，再点击「重新检查」。")
                }
            }
        }
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        refreshState()
    }

    private func openSafari() {
        guard let safari = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.Safari") else {
            report("未找到 Safari，请先确认 Safari 已安装。")
            return
        }
        NSWorkspace.shared.openApplication(at: safari, configuration: NSWorkspace.OpenConfiguration()) { _, error in
            if error != nil {
                DispatchQueue.main.async { self.report("无法启动 Safari，请从「应用程序」手动打开，再按 ⌘, 进入设置 → 扩展。") }
            }
        }
    }

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        switch message.body as? String {
        case "refresh-state": refreshState()
        case "open-safari": openSafari()
        case "open-preferences":
            SFSafariApplication.showPreferencesForExtension(withIdentifier: extensionBundleIdentifier) { error in
                DispatchQueue.main.async {
                    if let error = error {
                        let code = (error as NSError).code
                        self.report((error as NSError).domain == SFErrorDomain && code == SFErrorCode.noExtensionFound.rawValue
                            ? "Safari 尚未找到可用的 TDM 扩展，无法直接定位设置。请点击「打开 Safari」，按 ⌘, 进入设置 → 扩展。若没有「TDM 下载助手」，请按下方步骤允许未签名扩展，之后返回这里重新检查。"
                            : "Safari 未能打开扩展设置。请点击「打开 Safari」，按 ⌘, 手动进入设置 → 扩展，之后返回这里重新检查。")
                    } else {
                        self.refreshState()
                    }
                }
            }
        default: break
        }
    }
}
