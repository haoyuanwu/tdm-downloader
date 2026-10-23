import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { RenderBoundary } from "./components/RenderBoundary";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RenderBoundary><App /></RenderBoundary>
  </React.StrictMode>
);
