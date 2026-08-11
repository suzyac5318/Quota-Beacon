import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { DesignPlayground } from "./components/DesignPlayground";
import { PalettePreview } from "./components/PalettePreview";
import { PaletteEditor } from "./components/PaletteEditor";
import { AccountSwitcher } from "./components/AccountSwitcher";
import "./styles.css";

const params = new URLSearchParams(window.location.search);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{params.has("account-switcher") ? <AccountSwitcher /> : params.has("palette-editor") ? <PaletteEditor /> : params.has("palette") ? <PalettePreview /> : params.has("designer") ? <DesignPlayground /> : <App />}</React.StrictMode>,
);
