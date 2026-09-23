import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App";
import { installContextMenuGuard } from "./lib/contextMenu";
import "./styles/global.css";

installContextMenuGuard();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
