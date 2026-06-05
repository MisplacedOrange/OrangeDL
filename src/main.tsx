import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";
import "./styles/theme.css";
import "./styles/utilities.css";
import "./styles/layout.css";
import "./styles/downloads.css";
import "./styles/settings.css";
import "./styles/feedback.css";
import "./styles/responsive.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
