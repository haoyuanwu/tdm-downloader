(async () => {
  const { enabled = true, message = "尚未接管下载" } = await chrome.storage.local.get(["enabled", "message"]);
  document.querySelector("#enabled").checked = enabled;
  document.querySelector("#status").textContent = message;
  document.querySelector("#enabled").addEventListener("change", (event) => chrome.storage.local.set({ enabled: event.target.checked }));
})();
