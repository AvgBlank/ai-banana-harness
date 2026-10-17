import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { clientFor } from "./lib/client";
import { ClientProvider } from "./state/client-context";
import "./styles/index.css";

async function start(): Promise<void> {
  const root = document.getElementById("root");
  if (!root) throw new Error("the page has no #root element");
  const client = clientFor(window.banana);
  // The saved theme is applied before anything is drawn.
  const prefs = await client.prefs().catch(() => ({ theme: "dark" as const }));
  document.documentElement.dataset.theme = prefs.theme;
  createRoot(root).render(
    <StrictMode>
      <ClientProvider client={client}>
        <App theme={prefs.theme} />
      </ClientProvider>
    </StrictMode>,
  );
}

void start();
