import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  useRef,
} from "react";
import type { Prefs, PrefsPatch, Theme } from "../../shared/api";
import { useClient } from "./client-context";

export type { Theme };
export type View = "home" | "session" | "settings";

/** What the user is looking at; data from the sidecar lives in its own hooks. */
export interface UiState {
  theme: Theme;
  view: View;
  /** The workspace the home screen and title bar are about (a canonical path), if any. */
  workspace: string | null;
  sessionId: string | null;
  sidebarOpen: boolean;
  changesOpen: boolean;
  modelOpen: boolean;
  workspaceMenuOpen: boolean;
  draft: string;
  /** Why the last send failed; kept here because the composer is rebuilt when the view changes. */
  sendError: string | null;
}

export const initialState: UiState = {
  theme: "dark",
  view: "home",
  workspace: null,
  sessionId: null,
  sidebarOpen: true,
  changesOpen: false,
  modelOpen: false,
  workspaceMenuOpen: false,
  draft: "",
  sendError: null,
};

function reducer(state: UiState, patch: Partial<UiState>): UiState {
  return { ...state, ...patch };
}

interface Ctx {
  state: UiState;
  set: (patch: Partial<UiState>) => void;
}

const StateContext = createContext<Ctx | null>(null);

export function StateProvider({
  prefs,
  children,
}: {
  prefs: Pick<Prefs, "theme" | "sidebarOpen" | "changesOpen">;
  children: ReactNode;
}) {
  const client = useClient();
  const [state, dispatch] = useReducer(reducer, {
    ...initialState,
    theme: prefs.theme,
    sidebarOpen: prefs.sidebarOpen,
    changesOpen: prefs.changesOpen,
  });
  const set = useCallback((patch: Partial<UiState>) => dispatch(patch), []);

  useEffect(() => {
    document.documentElement.dataset.theme = state.theme;
  }, [state.theme]);
  // What main last stored; a change to any of these is handed to main to persist. A failed
  // save leaves the app on the choice.
  const saved = useRef<PrefsPatch>({
    theme: prefs.theme,
    sidebarOpen: prefs.sidebarOpen,
    changesOpen: prefs.changesOpen,
  });
  const { theme, sidebarOpen, changesOpen } = state;
  useEffect(() => {
    const now: PrefsPatch = { theme, sidebarOpen, changesOpen };
    const changed = Object.fromEntries(
      Object.entries(now).filter(
        ([key, value]) => saved.current[key as keyof PrefsPatch] !== value,
      ),
    ) as PrefsPatch;
    if (Object.keys(changed).length === 0) return;
    saved.current = now;
    void client.setPrefs(changed).catch(() => undefined);
  }, [client, theme, sidebarOpen, changesOpen]);

  const value = useMemo(() => ({ state, set }), [state, set]);
  return <StateContext.Provider value={value}>{children}</StateContext.Provider>;
}

export function useApp() {
  const ctx = useContext(StateContext);
  if (!ctx) throw new Error("useApp outside StateProvider");
  return ctx;
}
