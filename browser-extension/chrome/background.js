const HOST = "com.tdm.downloader.bridge";
const inFlight = new Set();
function supported(value) { try { const url = new URL(value); return ["http:", "https:"].includes(url.protocol) && !url.username && !url.password; } catch { return false; } }
async function state(message) { await chrome.storage.local.set({ message }); }
async function send(url, filename = "") {
  if (!supported(url)) throw new Error("仅支持普通 HTTP/HTTPS 文件链接");
  return chrome.runtime.sendNativeMessage(HOST, { url, filename });
}
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({ id: "tdm-download", title: "使用 TDM 下载此链接", contexts: ["link"] });
});
chrome.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId !== "tdm-download") return;
  try { const result = await send(info.linkUrl); await state(result.status === "accepted" ? "已加入 TDM 下载" : "未添加下载任务"); }
  catch (error) { await state(`连接失败：${error.message}。请安装或重新配置本地连接器。`); }
});
chrome.downloads.onCreated.addListener(async (item) => {
  const { enabled = true } = await chrome.storage.local.get("enabled");
  if (!enabled || item.byExtensionId || !supported(item.finalUrl || item.url) || item.state !== "in_progress" || inFlight.has(item.id)) return;
  inFlight.add(item.id);
  let paused = false;
  try {
    // 保留原任务，只有 TDM 成功创建任务并返回确认后才取消，失败或取消则续传。
    await chrome.downloads.pause(item.id); paused = true;
    await state("等待 TDM 确认，取消或超时后由浏览器继续下载");
    const result = await send(item.finalUrl || item.url, item.filename.split(/[\\/]/).pop());
    if (result.status === "accepted") {
      await chrome.downloads.cancel(item.id); paused = false;
      await state("已交给 TDM 下载");
    } else { await state("未交接，由浏览器继续下载"); }
  } catch (error) { await state(`接管失败，保留浏览器下载：${error.message}`); }
  finally {
    if (paused) await chrome.downloads.resume(item.id).catch(async () => state("原任务无法自动续传，请在 Chrome 下载列表中重试"));
    inFlight.delete(item.id);
  }
});
