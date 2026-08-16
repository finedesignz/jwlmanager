import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { SettingsProvider } from "./settings/SettingsProvider";
import "./styles.css";

// `SettingsProvider` is lifted ABOVE `App` here (rather than nested inside
// it) so its own error banner -- surfaced on a rejected `save_settings` --
// reuses the app's existing `ErrorBanner`/`describeError` path without
// needing `App.tsx` to know settings persistence exists (D11-04,
// 11-01-PLAN.md Task 1).
ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <SettingsProvider>
      <App />
    </SettingsProvider>
  </React.StrictMode>,
);
