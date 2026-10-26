import { useState } from "react";
import { api } from "../api";
import { isWindows } from "../platform";
import { ChromeExtensionManager } from "./ChromeExtensionManager";
import { SafariExtensionGuide } from "./SafariExtensionGuide";

export function BrowserExtensionGuide() {
  const [browser, setBrowser] = useState(isWindows ? "chrome" : "safari");
  const [copyMessage, setCopyMessage] = useState("");
  const copyAddress = async (edge = false) => {
    try {
      await api.copyText(edge ? "edge://extensions" : "chrome://extensions");
      setCopyMessage(`已复制，请粘贴到 ${edge ? "Edge" : "Chrome"} 地址栏并回车。`);
    } catch {
      setCopyMessage("复制失败，请手动在 Chrome 地址栏输入 chrome://extensions。");
    }
  };

  return <>
    <div className="browser-guide-switch" role="group" aria-label="选择浏览器教程">
      {(isWindows ? [["chrome", "Chrome / Edge"]] : [["safari", "Safari"], ["chrome", "Google Chrome"]]).map(([id, name]) =>
        <button type="button" key={id} className={`btn ${browser === id ? "is-selected" : ""}`} aria-pressed={browser === id} aria-controls={`browser-guide-${id}`} onClick={() => setBrowser(id)}>{name}</button>)}
    </div>
    {!isWindows && <div id="browser-guide-safari" hidden={browser !== "safari"}><SafariExtensionGuide /></div>}
    <section id="browser-guide-chrome" hidden={browser !== "chrome"} aria-labelledby="chrome-guide-title">
      <h3 className="section-title" id="chrome-guide-title">{isWindows ? "Chrome / Edge 扩展" : "Chrome 扩展"}</h3>
      <p className="dialog-subtitle">触发浏览器下载后打开 TDM，确认后交给下载器处理。</p>
      <div className="safari-guide-action">
        <div><strong>TDM 下载助手 · 本地试用版</strong><p className="hint">当前通过「加载已解压的扩展程序」安装，需要扩展文件与本地连接器。</p></div>
        <button className="btn" type="button" onClick={() => void copyAddress()}>复制 Chrome 扩展管理页地址</button>
        {isWindows && <button className="btn" type="button" onClick={() => void copyAddress(true)}>复制 Edge 扩展管理页地址</button>}
      </div>
      <ChromeExtensionManager />
      {copyMessage && <p className="hint" role="status">{copyMessage}</p>}
      <div className="browser-launch-help">
        <strong>如何打开与开启插件</strong>
        <p>完成安装后，在 Chrome 扩展管理页启用「TDM 下载助手」。回到 Chrome，按 <kbd>{isWindows ? "Ctrl + Shift + Y" : "⇧⌘Y"}</kbd>{!isWindows && "（Command + Shift + Y）"} 打开助手弹窗，打开「自动接管下载」开关。</p>
        <p>也可点击工具栏的拼图图标 →「TDM 下载助手」，点击图钉可固定到工具栏。快捷键用于打开弹窗，不会替你安装或启用尚未开启的扩展。</p>
        <p>更新扩展后请重新加载。若保留了旧快捷键或发生冲突，在 Chrome 地址栏打开 <code>chrome://extensions/shortcuts</code>（Edge 为 <code>edge://extensions/shortcuts</code>），为「激活此扩展程序」设置上述快捷键。</p>
        <p>助手弹窗会显示「TDM 下载助手 · 打开助手」实际分配的键位；若提示快捷键未分配、可能被占用，点击提示下方的「快捷键不可用 · 点击前往设置」，即可跳转到当前浏览器的快捷键设置，找到「TDM 下载助手 → 激活此扩展程序」重新分配。Windows 与 macOS 操作一致。</p>
      </div>
      <ol className="safari-guide-steps">
        <li><strong>一键准备扩展文件</strong><p>点击上方「一键准备插件」，再点击「打开插件文件夹」。插件与连接器会保存到固定目录，后续 App 更新不会改变这个路径。无需自行编译连接器。</p></li>
        <li><strong>在 Chrome 中加载扩展</strong><p>在 Chrome 地址栏输入 <code>chrome://extensions</code>，打开右上角「开发者模式」，点击「加载已解压的扩展程序」，选择上一步的扩展文件夹。安装后保持文件夹位置不变。</p></li>
        <li><strong>复制扩展 ID</strong><p>找到「TDM 下载助手」，复制卡片上的 32 位扩展 ID。重新加载不同文件夹时，ID 可能变化，应以当前显示的 ID 为准。</p></li>
        <li><strong>一键连接浏览器</strong><p>把 ID 粘贴到上方输入框，点击「{isWindows ? "连接浏览器" : "连接 Chrome"}」，软件会配置本地连接器并保存扩展 ID。首次配置后，日常 App 覆盖更新无需重新填写。插件版本变化时点击「检查并更新插件」，再到浏览器重新加载扩展。</p></li>
        <li><strong>重新加载并测试下载</strong><p>返回扩展管理页，点击「TDM 下载助手」的重新加载按钮。在工具栏扩展菜单中打开它，确认下载接管已开启，再点击普通 HTTP / HTTPS 文件的下载链接。TDM 弹出确认窗后，点击「下载」。</p></li>
      </ol>
      {isWindows && <p className="hint">Edge 使用同一份插件文件，在 edge://extensions 加载；若 Edge 显示不同的扩展 ID，再填写该 ID 连接一次，已连接的 Chrome ID 会保留。仅配置当前用户，无需管理员权限。</p>}
      <details className="safari-guide-details"><summary>点击下载后没有打开 TDM？</summary><p>先检查扩展弹窗中的最近状态。确认 TDM 可以独立打开、连接器已安装、扩展 ID 与上方填写的一致。更换扩展 ID 后点击「连接 Chrome」重新配置；移动 App 后先打开一次 TDM，以同步连接目标。</p><p>若仍在运行旧版 TDM，请先退出旧版，再打开支持浏览器接管的新版。</p></details>
      <details className="safari-guide-details"><summary>取消接管与支持范围</summary><p>可在扩展弹窗中关闭下载接管。取消 TDM 确认窗、连接失败或等待超时后，扩展会尝试恢复 Chrome 原下载；如服务器不支持续传，请在 Chrome 下载列表中重试。</p><p>目前支持普通 HTTP / HTTPS 文件下载，不支持 blob / data 链接；需要登录 Cookie 或特殊请求头的资源可能无法由 TDM 下载。极小文件可能在接管前已由 Chrome 完成。</p></details>
      <p className="hint">教程中的操作立即执行，无需保存设置。</p>
    </section>
  </>;
}
