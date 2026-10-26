const { test, before, after } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const { spawnSync } = require("node:child_process");

const windows = process.platform === "win32";
let directory, host, env;
before(() => {
  if (!windows) return;
  directory = fs.mkdtempSync(path.join(os.tmpdir(), "tdm-bridge-test-"));
  const binaries = path.join(directory, "中文 app folder");
  fs.mkdirSync(binaries);
  const app = path.join(binaries, "fake-app.exe");
  const source = path.join(directory, "app.rs");
  fs.writeFileSync(source, `fn main() {
    let request = std::env::args().nth(1).unwrap();
    let root = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap());
    std::fs::write(root.join("last-request.txt"), &request).unwrap();
    let id = request.split("request=").nth(1).unwrap().split('&').next().unwrap();
    let decision = root.join("TDM Browser Bridge/decisions").join(id);
    std::fs::write(decision, std::env::var("TDM_TEST_DECISION").unwrap()).unwrap();
  }`);
  const build = spawnSync("rustc", [source, "-o", app], { encoding: "utf8", windowsHide: true });
  assert.equal(build.status, 0, build.stderr);
  host = path.join(binaries, "tdm-browser-bridge.exe");
  fs.copyFileSync(path.resolve(__dirname, "../../release/browser-native-windows/tdm-browser-bridge.exe"), host);
  fs.writeFileSync(path.join(binaries, "app-path.txt"), app);
  env = { ...process.env, LOCALAPPDATA: directory };
});
after(() => {
  if (!directory) return;
  assert.equal(path.dirname(path.resolve(directory)), path.resolve(os.tmpdir()));
  assert.ok(path.basename(directory).startsWith("tdm-bridge-test-"));
  fs.rmSync(directory, { recursive: true, force: true });
});

function frame(value) {
  const data = Buffer.from(JSON.stringify(value));
  const size = Buffer.alloc(4);
  size.writeUInt32LE(data.length);
  return Buffer.concat([size, data]);
}
function run(input, decision = "accepted") {
  const result = spawnSync(host, [], { input, env: { ...env, TDM_TEST_DECISION: decision }, timeout: 8000, windowsHide: true });
  assert.equal(result.status, 0, result.stderr?.toString());
  assert.equal(result.stdout.length - 4, result.stdout.readUInt32LE(0), "stdout must contain exactly one Native Messaging frame");
  return JSON.parse(result.stdout.subarray(4).toString());
}

test("native host launches paths with spaces and Unicode, returns acceptance, and cleans decisions", { skip: !windows }, () => {
  const url = "https://example.com/文件.zip?a=1&b=hello%20world";
  const filename = "下载 文件.zip";
  assert.equal(run(frame({ url, filename })).status, "accepted");
  const request = new URL(fs.readFileSync(path.join(directory, "last-request.txt"), "utf8"));
  assert.equal(request.searchParams.get("url"), new URL(url).href);
  assert.equal(request.searchParams.get("filename"), filename);
  assert.deepEqual(fs.readdirSync(path.join(directory, "TDM Browser Bridge/decisions")), []);
});
test("native cancellation is returned to the browser without claiming acceptance", { skip: !windows }, () => {
  assert.equal(run(frame({ url: "https://example.com/file.zip" }), "cancelled").status, "cancelled");
});
test("native host rejects local URLs, credentials, malformed frames, and missing app configuration", { skip: !windows }, () => {
  for (const input of [frame({ url: "file:///C:/Windows/notepad.exe" }), frame({ url: "https://user:pass@example.com" }), Buffer.from([0, 0, 1, 0]), Buffer.from([4, 0, 0, 0, 123])]) {
    assert.equal(run(input).status, "error");
  }
  fs.renameSync(path.join(path.dirname(host), "app-path.txt"), path.join(path.dirname(host), "saved-path.txt"));
  assert.equal(run(frame({ url: "https://example.com/file.zip" })).status, "error");
});
