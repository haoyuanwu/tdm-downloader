(async () => {
  const help = document.querySelector("#shortcut-help");
  const settings = document.querySelector("#shortcut-settings");
  const settingsError = document.querySelector("#shortcut-error");
  const settingsUrl = /Edg\//.test(navigator.userAgent) ? "edge://extensions/shortcuts" : "chrome://extensions/shortcuts";
  settings.addEventListener("click", async () => {
    settingsError.textContent = "";
    try {
      await chrome.tabs.create({ url: settingsUrl });
    } catch {
      settingsError.textContent = `无法打开设置，请在地址栏输入 ${settingsUrl}，找到「TDM 下载助手 → 激活此扩展程序」。`;
    }
  });
  async function refreshShortcut() {
    try {
      const [platform, commands] = await Promise.all([chrome.runtime.getPlatformInfo(), chrome.commands.getAll()]);
      const suggested = platform.os === "mac" ? "Command + Shift + Y（⇧⌘Y）" : "Ctrl + Shift + Y";
      const command = commands.find(({ name }) => name === "_execute_action");
      const shortcut = command?.shortcut?.trim();
      help.classList.toggle("shortcut-warning", !!command && !shortcut);
      if (shortcut) {
        help.textContent = `TDM 下载助手 · 打开助手：${shortcut}`;
        settings.textContent = "修改快捷键";
      } else if (command) {
        help.textContent = `TDM 下载助手 · 打开助手：快捷键未分配，可能被占用或已清除。建议键位：${suggested}。`;
        settings.textContent = "快捷键不可用 · 点击前往设置";
      } else {
        help.textContent = `TDM 下载助手 · 打开助手：无法读取当前快捷键，请在设置中确认。默认键位：${suggested}。`;
        settings.textContent = "查看快捷键设置";
      }
    } catch {
      help.textContent = "无法读取「TDM 下载助手 · 打开助手」的快捷键，请在设置中确认。";
      help.classList.remove("shortcut-warning");
      settings.textContent = "查看快捷键设置";
    }
  }
  window.addEventListener("focus", refreshShortcut);
  void refreshShortcut();
  const { enabled = true, message = "尚未接管下载" } = await chrome.storage.local.get(["enabled", "message"]);
  document.querySelector("#enabled").checked = enabled;
  document.querySelector("#status").textContent = message;
  document.querySelector("#enabled").addEventListener("change", (event) => chrome.storage.local.set({ enabled: event.target.checked }));
})();
