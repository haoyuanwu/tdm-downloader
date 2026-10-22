/** WebView 的系统标识用于平台用语；实际系统操作由后端完成。 */
export const isWindows = navigator.userAgent.includes("Windows");
export const trashName = isWindows ? "回收站" : "废纸篓";
