(() => {
  const file = /\.(?:zip|7z|rar|tar|gz|bz2|xz|dmg|pkg|exe|msi|iso|pdf|mp4|mp3|mov|mkv|gguf|bin|csv|xlsx|docx)(?:$|[?#])/i;
  document.addEventListener("click", async (event) => {
    if (!event.isTrusted || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || event.defaultPrevented) return;
    const anchor = event.composedPath().find(node => node instanceof HTMLAnchorElement);
    if (!anchor || !(anchor.hasAttribute("download") || file.test(anchor.href))) return;
    const url = new URL(anchor.href);
    if (!["http:","https:"].includes(url.protocol) || url.username || url.password) return;
    event.preventDefault();
    const fallback = () => { anchor.click(); };
    try {
      const {enabled = true} = await browser.storage.local.get("enabled");
      if (!enabled) { fallback(); return; }
      const result = await browser.runtime.sendMessage({type:"download",url:url.href,filename:anchor.getAttribute("download") || ""});
      if (!result?.ok) fallback();
    } catch { fallback(); }
  }, true);
})();
