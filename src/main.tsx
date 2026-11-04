import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { ApiProvider } from "./components/ApiProvider";
import { initializeTheme } from "./lib/theme";
import "./styles.css";
import "./styles-cleanup.css";
import "./styles-shell.css";
import "./styles-phase11.css";
import "./styles-phase23.css";
import "./styles-phase27.css";
import "./styles-phase30.css";
import "./styles-phase31.css";
import "./styles-phase32.css";
import "./styles-phase33.css";
import "./styles-phase34.css";
import "./styles-phase35.css";
import "./styles-design-system.css";
import "./styles-home.css";
import "./styles-simplified.css";

const rootElement = document.getElementById("root");
if (rootElement === null) {
  throw new Error("SentinelLite UI root element is missing");
}

initializeTheme();

createRoot(rootElement).render(
  <StrictMode>
    <ApiProvider>
      <App />
    </ApiProvider>
  </StrictMode>,
);
