import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/app.css";

const root = document.getElementById("root");

if (!root) {
  throw new Error("找不到根节点 #root");
}

if (!(window as Window & { __BACKUP_STARTUP_ERROR__?: string | null }).__BACKUP_STARTUP_ERROR__) {
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}
