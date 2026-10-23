const api = browser;
// Recreate cached menu items on background startup and extension updates.
let menuReady;
function ensureContextMenu() {
  if (!menuReady) {
    menuReady = api.contextMenus.removeAll()
      .then(() => api.contextMenus.create({id:"tdm-download",title:"\u4f7f\u7528 TDM \u4e0b\u8f7d\u6b64\u94fe\u63a5",contexts:["link"]}))
      .catch(error => { menuReady = undefined; console.warn("TDM context menu:", error); });
  }
  return menuReady;
}
api.runtime.onInstalled.addListener(ensureContextMenu);
void ensureContextMenu();
async function handoff(url, filename) {
  return api.runtime.sendNativeMessage("com.tdm.downloader.safari", {url,filename:filename || ""});
}
api.runtime.onMessage.addListener((message, sender) => {
  if (message.type !== "download" || !sender.tab) return;
  return handoff(message.url, message.filename);
});
api.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId !== "tdm-download") return;
  try { const result = await handoff(info.linkUrl); await api.storage.local.set({message:result.ok ? "\u5df2\u5524\u8d77 TDM\uff0c\u8bf7\u5728\u5e94\u7528\u4e2d\u786e\u8ba4" : result.error}); }
  catch (error) { await api.storage.local.set({message:`\u8fde\u63a5\u5931\u8d25\uff1a${error.message}`}); }
});
