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
import type { Theme } from "../../shared/api";
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
};

function reducer(state: UiState, patch: Partial<UiState>): UiState {
  return { ...state, ...patch };
}

interface Ctx {
  state: UiState;
  set: (patch: Partial<UiState>) => void;
}

const StateContext = createContext<Ctx | null>(null);

export function StateProvider({ theme, children }: { theme: Theme; children: ReactNode }) {
  const client = useClient();
  const [state, dispatch] = useReducer(reducer, { ...initialState, theme });
  const set = useCallback((patch: Partial<UiState>) => dispatch(patch), []);

  // Applied at once, and handed to main to persist; a failed save leaves the app on the choice.
  useEffect(() => {
    document.documentElement.dataset.theme = state.theme;
  }, [state.theme]);
  const saved = useRef(theme);
  const { theme: current } = state;
  useEffect(() => {
    if (current === saved.current) return;
    saved.current = current;
    void client.setPrefs({ theme: current }).catch(() => undefined);
  }, [client, current]);

  const value = useMemo(() => ({ state, set }), [state, set]);
  return <StateContext.Provider value={value}>{children}</StateContext.Provider>;
}

export function useApp() {
  const ctx = useContext(StateContext);
  if (!ctx) throw new Error("useApp outside StateProvider");
  return ctx;
}
