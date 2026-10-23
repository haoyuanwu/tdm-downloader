import { useEffect, useState } from "react";
import { api } from "../api";
import { Icon } from "./Icon";
import { isWindows } from "../platform";

export function SafariExtensionGuide() {
  const [version, setVersion] = useState("");
  useEffect(() => { void api.browserExtensionStatus().then((status) => setVersion(status.safari_version)).catch(() => {}); }, []);
  const [opening, setOpening] = useState(false);
  const [message, setMessage] = useState("");
  const [failed, setFailed] = useState(false);
  const openConnector = async () => {
    setOpening(true);
    setMessage("");
    setFailed(false);
    try {
      await api.openSafariConnector();
      setMessage("已请求打开安装器，请在安装器中点击「打开 Safari 扩展设置」。");
    } catch (error) {
      setFailed(true);
      setMessage(String(error));
    } finally {
      setOpening(false);
    }
  };
  return <section className="safari-guide" aria-labelledby="safari-guide-title">
    <h3 className="section-title" id="safari-guide-title">Safari 扩展</h3>
    <p className="dialog-subtitle">点击网页中的下载链接，在 TDM 中确认后开始下载。</p>
    <div className="safari-guide-action">
      <div><strong>TDM Safari Connector</strong><p className="hint">{version ? `Safari 插件 v${version}，版本与 App 独立。` : "扩展版本与 App 独立。"}安装器固定放在用户 Applications 目录，避免同名副本；首次使用需要手动启用。</p></div>
      <button className="btn btn-primary" type="button" disabled={opening || isWindows} onClick={openConnector}><Icon name="external" size={16} />{opening ? "正在打开…" : "打开扩展安装器"}</button>
    </div>
    {isWindows && <p className="hint">Safari 扩展仅支持 macOS。</p>}
    {message && <p className={failed ? "error-text" : "hint"} role={failed ? "alert" : "status"}>{message}</p>}
    <div className="browser-launch-help">
      <strong>如何打开与开启插件</strong>
      <p>先在 Safari → 设置 → 扩展中勾选「TDM 下载助手」并授权网站。回到 Safari，按 <kbd>⇧⌘D</kbd>（Shift + Command + D）打开助手弹窗，打开「自动接管下载」开关。</p>
      <p>也可点击 Safari 工具栏中的 TDM 扩展图标；图标未显示时，右键工具栏 → 自定工具栏，将 TDM 图标拖入工具栏。</p>
      <p>快捷键用于打开插件弹窗，首次启用与网站授权仍需在 Safari 设置中完成。更新后需重新载入扩展；若按键被其他功能占用，可先点击工具栏图标。</p>
    </div>
    <p className="hint">App 正常覆盖更新、Safari 扩展版本不变时，通常无需重新启用；版本变化时点击上方安装器按钮完成更新。网站权限和浏览器要求的启用步骤仍在 Safari 中完成。</p>
    <ol className="safari-guide-steps">
      <li><strong>打开扩展设置</strong><p>点击上方按钮，再在安装器中点击「打开 Safari 扩展设置」。也可以进入 Safari → 设置（旧版为偏好设置）→ 扩展。</p></li>
      <li><strong>启用 TDM 扩展</strong><p>勾选「TDM 下载助手」（安装器名称为 TDM Safari Connector）。此试用版使用临时签名；若扩展未出现或无法启用，请按下方说明允许未签名扩展，再重新打开安装器。</p></li>
      <li><strong>允许访问下载网站</strong><p>在扩展设置中打开网站访问权限，允许要下载的网站。随后刷新该网页，确保扩展开始工作。</p></li>
      <li><strong>试一次下载</strong><p>点击指向 .zip、.dmg、.pdf 等文件的链接，或右键链接选择「使用 TDM 下载此链接」。TDM 会打开下载确认窗，检查地址后点击「下载」。</p></li>
    </ol>
    <details className="safari-guide-details"><summary>临时签名版：如何允许未签名扩展？</summary><p>在 Safari → 设置 → 高级中开启「显示网页开发者功能」（旧版为「在菜单栏中显示开发菜单」），然后在「开发」菜单中勾选「允许未签名的扩展」。部分版本将此选项放在开发者设置中。</p><p>Safari 重新启动后可能需要再次允许。正式的 Developer ID 签名与 Apple 公证版本才能免除此试用步骤；此开关由你在 Safari 中手动设置。</p></details>
    <details className="safari-guide-details"><summary>无法打开扩展设置 / SFErrorDomain 1？</summary><p>这表示 Safari 尚未找到可用的 TDM 扩展。打开 Safari，按 ⌘, 进入设置 → 扩展：若能看到「TDM 下载助手」，先启用它；若看不到，按上方临时签名教程手动允许未签名扩展，再返回安装器点击「重新检查」。</p><p>系统注册了扩展，不代表 Safari 已接受该临时签名扩展。若已允许未签名扩展仍不显示，需要用 Apple Development 证书进行本机测试，或使用 Developer ID 发行签名版本。安装器会保留说明与重试入口，不会自动更改 Safari 安全设置。</p></details>
    <details className="safari-guide-details"><summary>没有弹出下载确认窗？</summary><p>确认 TDM 能正常打开、扩展已启用并获得当前网站权限，刷新网页后重试。可以先用右键菜单测试普通的 HTTP / HTTPS 文件链接。</p><p>目前不支持 blob 链接、表单提交和部分由网页脚本生成的下载；需要网站登录信息的文件也可能无法下载。在 TDM 中取消后，需要回到网页重新点击，Safari 不会自动接续该下载。</p></details>
    <p className="hint">教程中的操作立即执行，无需保存设置。</p>
  </section>;
}
