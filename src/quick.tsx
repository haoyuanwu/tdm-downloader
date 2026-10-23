import React from "react";
import ReactDOM from "react-dom/client";
import QuickDownload from "./QuickDownload";
import { RenderBoundary } from "./components/RenderBoundary";
import "./styles.css";
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode><RenderBoundary><QuickDownload /></RenderBoundary></React.StrictMode>
);
