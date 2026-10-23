(async () => {
  const { enabled = true, message = "\u5c1a\u672a\u63a5\u7ba1\u4e0b\u8f7d" } = await browser.storage.local.get(["enabled", "message"]);
  document.querySelector("#enabled").checked = enabled;
  document.querySelector("#status").textContent = message;
  document.querySelector("#enabled").addEventListener("change", (event) => browser.storage.local.set({ enabled: event.target.checked }));
})();
