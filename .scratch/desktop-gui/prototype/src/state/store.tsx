import { createContext, useCallback, useContext, useEffect, useMemo, useReducer, type ReactNode } from "react";
import type { ProviderId } from "@/mock/data";

export type Theme = "system" | "dark" | "light";
export type View = "home" | "session" | "settings";
export type DialogId = null | "approval" | "trust" | "quit" | "link";
export type SidecarState = "ready" | "crashed" | "failed";

export interface AppState {
  theme: Theme;
  view: View;
  workspaceId: string;
  sessionId: string | null;
  sidebarOpen: boolean;
  changesOpen: boolean;
  changesScope: "turn" | "workspace";
  changesEmpty: boolean;
  selectedFile: string;
  streaming: boolean;
  streamRun: number;
  dialog: DialogId;
  modelOpen: boolean;
  workspaceMenuOpen: boolean;
  provider: ProviderId;
  keyState: "present" | "missing";
  sidecar: SidecarState;
  trustedWorkspaces: string[];
}

export const initialState: AppState = {
  theme: "dark",
  view: "home",
  workspaceId: "ws-harness",
  sessionId: null,
  sidebarOpen: true,
  changesOpen: false,
  changesScope: "turn",
  changesEmpty: false,
  selectedFile: "crates/harness-core/src/changes.rs",
  streaming: false,
  streamRun: 0,
  dialog: null,
  modelOpen: false,
  workspaceMenuOpen: false,
  provider: "deepseek",
  keyState: "present",
  sidecar: "ready",
  trustedWorkspaces: ["ws-harness"],
};

type Action = { type: "patch"; patch: Partial<AppState> };

function reducer(state: AppState, action: Action): AppState {
  return { ...state, ...action.patch };
}

interface Ctx {
  state: AppState;
  set: (patch: Partial<AppState>) => void;
}

const StateContext = createContext<Ctx | null>(null);

function readTheme(): Theme {
  try {
    const t = localStorage.getItem("bh-theme");
    if (t === "system" || t === "dark" || t === "light") return t;
  } catch {
    /* storage unavailable: fall back to dark */
  }
  return "dark";
}

export function StateProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, { ...initialState, theme: readTheme() });
  const set = useCallback((patch: Partial<AppState>) => dispatch({ type: "patch", patch }), []);

  useEffect(() => {
    document.documentElement.dataset.theme = state.theme;
    try {
      localStorage.setItem("bh-theme", state.theme);
    } catch {
      /* ignore */
    }
  }, [state.theme]);

  const value = useMemo(() => ({ state, set }), [state, set]);
  return <StateContext.Provider value={value}>{children}</StateContext.Provider>;
}

export function useApp() {
  const ctx = useContext(StateContext);
  if (!ctx) throw new Error("useApp outside StateProvider");
  return ctx;
}
