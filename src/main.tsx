import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ClipboardPopup } from "./components/ClipboardPopup";
import { QuickNotePopup } from "./components/QuickNotePopup";
import "./styles/global.css";

function Root() {
  const path = window.location.pathname;
  if (path === "/clipboard-popup") return <ClipboardPopup />;
  if (path === "/quick-note") return <QuickNotePopup />;
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>
);
