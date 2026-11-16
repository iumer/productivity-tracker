import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./index.css";
import "./App.css";
import App from "./App";
import { NudgeToast } from "./components/NudgeToast";

const label = getCurrentWindow().label;

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {label === "nudge" ? <NudgeToast /> : <App />}
  </StrictMode>,
);
