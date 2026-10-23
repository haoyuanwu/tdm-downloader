import AppKit
import Foundation

func reply(_ object: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: object) else { return }
    var count = UInt32(data.count).littleEndian
    FileHandle.standardOutput.write(Data(bytes: &count, count: 4))
    FileHandle.standardOutput.write(data)
}
func readExactly(_ length: Int) -> Data {
    var data = Data()
    while data.count < length {
        let part = FileHandle.standardInput.readData(ofLength: length - data.count)
        if part.isEmpty { break }
        data.append(part)
    }
    return data
}
let prefix = readExactly(4)
guard prefix.count == 4 else { exit(1) }
let size = prefix.enumerated().reduce(UInt32(0)) { $0 | UInt32($1.element) << ($1.offset * 8) }
guard size > 0 && size <= 32768,
      let message = try? JSONSerialization.jsonObject(with: readExactly(Int(size))) as? [String: Any],
      let address = message["url"] as? String, let target = URL(string: address),
      ["http", "https"].contains(target.scheme ?? ""), target.host != nil,
      target.user == nil, target.password == nil else { reply(["status": "error", "error": "仅支持 HTTP/HTTPS 下载链接"]); exit(0) }
// A debug Tauri app loads its UI from Vite, not from the application bundle.
// Never open that app while Vite is offline: it would only show an empty window.
let config = URL(fileURLWithPath: CommandLine.arguments[0]).deletingLastPathComponent().appendingPathComponent("app-path.txt")
guard let appPath = (try? String(contentsOf: config, encoding: .utf8))?.trimmingCharacters(in: .whitespacesAndNewlines),
      let bundle = Bundle(path: appPath) else {
    reply(["status": "error", "error": "找不到配置的 TDM 应用，请重新安装本地连接器"]); exit(0)
}
let development = bundle.bundleIdentifier == "com.tdm.downloader.dev"
if development {
    let done = DispatchSemaphore(value: 0)
    var available = false
    var request = URLRequest(url: URL(string: "http://127.0.0.1:1420/@vite/client")!)
    request.timeoutInterval = 2
    let task = URLSession.shared.dataTask(with: request) { _, response, _ in
        available = (response as? HTTPURLResponse)?.statusCode == 200
        done.signal()
    }
    task.resume()
    let completed = done.wait(timeout: .now() + 3) == .success
    task.cancel()
    guard completed && available else {
        reply(["status": "error", "error": "连接器指向开发版，但开发服务器未运行。请将连接器重新配置到正式版 TDM 应用"]); exit(0)
    }
}
let id = UUID().uuidString.lowercased()
let cache = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Caches/com.tdm.downloader.bridge")
try? FileManager.default.createDirectory(at: cache, withIntermediateDirectories: true)
let decision = cache.appendingPathComponent(id)
defer { try? FileManager.default.removeItem(at: decision) }
var url = URLComponents()
url.scheme = development ? "tdm-dev" : "tdm"; url.host = "download"
url.queryItems = [URLQueryItem(name: "url", value: address), URLQueryItem(name: "request", value: id), URLQueryItem(name: "filename", value: message["filename"] as? String ?? "")]
guard let link = url.url else { reply(["status": "error"]); exit(0) }
let process = Process()
process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
// The installer pins the exact application path, avoiding an older installed copy.
process.arguments = ["-a", appPath, link.absoluteString]
process.standardOutput = FileHandle.standardError
process.standardError = FileHandle.standardError
do { try process.run(); process.waitUntilExit() } catch { reply(["status": "error", "error": "无法启动 TDM"]); exit(0) }
guard process.terminationStatus == 0 else { reply(["status": "error", "error": "未找到 TDM，请重新安装连接器"]); exit(0) }
let deadline = Date().addingTimeInterval(120)
while Date() < deadline {
    if let result = try? String(contentsOf: decision, encoding: .utf8) {
        reply(["status": result]); exit(0)
    }
    Thread.sleep(forTimeInterval: 0.25)
}
reply(["status": "timeout", "error": "确认超时，浏览器继续下载"])
