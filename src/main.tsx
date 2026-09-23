import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App";
import { bootstrapTheme } from "./lib/colorScheme";
import { installContextMenuGuard } from "./lib/contextMenu";
import "./styles/global.css";

installContextMenuGuard();
bootstrapTheme();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
