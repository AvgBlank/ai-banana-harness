import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import type { Prefs } from "../shared/api";
import { App } from "./App";
import { clientFor } from "./lib/client";
import { ClientProvider } from "./state/client-context";
import "./styles/index.css";

const DEFAULT_PREFS: Prefs = {
  theme: "dark",
  sidebarOpen: true,
  changesOpen: false,
  changesScope: "turn",
  changesWidth: 480,
  lastWorkspace: null,
  recents: [],
};

async function start(): Promise<void> {
  const root = document.getElementById("root");
  if (!root) throw new Error("the page has no #root element");
  const client = clientFor(window.banana);
  // The saved theme is applied before anything is drawn.
  const prefs = await client.prefs().catch(() => DEFAULT_PREFS);
  document.documentElement.dataset.theme = prefs.theme;
  createRoot(root).render(
    <StrictMode>
      <ClientProvider client={client}>
        <App prefs={prefs} />
      </ClientProvider>
    </StrictMode>,
  );
}

void start();
