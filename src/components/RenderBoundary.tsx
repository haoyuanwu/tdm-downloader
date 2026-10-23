import { Component } from "react";
import type { ReactNode, ErrorInfo } from "react";
import { invoke } from "@tauri-apps/api/core";
export class RenderBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(error: Error, _info: ErrorInfo) {
    void invoke("frontend_health", { label: "render-error", message: error.message }).catch(() => {});
    void invoke("quick_window_ready").catch(() => {});
  }
  render() {
    return this.state.failed ? <main style={{ padding: 20, fontSize: 13 }}><h2 style={{fontSize:16}}>界面暂时无法加载</h2><p>请重新加载。若仍失败，请退出后重新打开应用。</p><button className="btn" onClick={() => window.location.reload()}>重新加载</button></main> : this.props.children;
  }
}
