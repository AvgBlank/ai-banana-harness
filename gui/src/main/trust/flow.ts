import type { OpenedWorkspace, TrustReview } from "../../shared/api";
import { IpcError } from "../../shared/ipc";
import type { SidecarSupervisor } from "../sidecar/supervisor";
import { checkDestinations, type Destination, originOf } from "./hostcheck";
import type { TrustStore } from "./store";
import { type ConfigFile, readConfig, summarize } from "./summary";

export interface TrustFlowDeps {
  request: SidecarSupervisor["request"];
  store: TrustStore;
  /** Reads and hashes the workspace config; replaceable only so tests need no real files. */
  readConfig?: (path: string) => Promise<ConfigFile>;
  /** Told whenever a workspace becomes open or closed, so main's open list stays right. */
  opened: (workspace: string, open: boolean) => void;
}

const BLOCKED = "Blocked: this folder's config changes where your API key is sent.";

/**
 * Restricted first, then an upgrade only for a config file the user trusted: opening, reviewing,
 * trusting and forgetting. The renderer supplies a path and a reviewed hash, never file content
 * or a trust flag.
 */
export class WorkspaceTrust {
  private readonly read: (path: string) => Promise<ConfigFile>;

  constructor(private readonly deps: TrustFlowDeps) {
    this.read = deps.readConfig ?? readConfig;
  }

  async open(path: string): Promise<OpenedWorkspace> {
    const first = await this.openRestricted(path);
    const config = first.workspace_config;
    if (!config.present) return { workspace: first.workspace, trust: "none" };
    // An open workspace keeps the config it was opened with, even when the file changed since.
    if (config.loaded) return { workspace: first.workspace, trust: "trusted" };

    const file = await this.read(config.path);
    if (!file.ok) return { workspace: first.workspace, trust: "unreadable" };
    const entry = await this.deps.store.get(first.workspace);
    const matches = entry?.configSha256 === file.sha256;
    if (matches && !(await this.turnRunning(first.workspace))) {
      try {
        return await this.upgrade({ workspace: first.workspace, approved: entry.approvedHosts });
      } catch (error) {
        if (error instanceof IpcError && error.code === "config_invalid") {
          return { workspace: first.workspace, trust: "unreadable" };
        }
        throw error;
      }
    }
    return { workspace: first.workspace, trust: "needs_review" };
  }

  async review(path: string): Promise<TrustReview> {
    const first = await this.openRestricted(path);
    const file = await this.read(first.workspace_config.path);
    if (!file.ok)
      throw new IpcError("config_unreadable", `the config cannot be read (${file.reason})`);
    const baseline = await this.destinations(first.workspace);
    const parsed = summarize(file.text, baseline);
    return {
      workspace: first.workspace,
      configPath: first.workspace_config.path,
      sha256: file.sha256,
      raw: file.text,
      parseError: parsed.error,
      summary: parsed.summary,
      baseline: baseline.map((d) => ({ provider: d.provider, origin: originOf(d) })),
    };
  }

  async trust(options: {
    path: string;
    sha256: string;
    acknowledgeHosts: boolean;
  }): Promise<OpenedWorkspace> {
    const first = await this.openRestricted(options.path);
    const { workspace } = first;
    if (first.workspace_config.loaded) return { workspace, trust: "trusted" };
    const file = await this.read(first.workspace_config.path);
    if (!file.ok)
      throw new IpcError("config_unreadable", `the config cannot be read (${file.reason})`);
    if (file.sha256 !== options.sha256) {
      throw new IpcError("config_changed", "the config changed since you reviewed it");
    }
    const parsed = summarize(file.text, await this.destinations(workspace));
    if (!parsed.summary) throw new IpcError("config_invalid", "the config does not parse");
    const approved = parsed.summary.providers.flatMap((p) =>
      p.differs && p.origin ? [p.origin] : [],
    );
    if (approved.length > 0 && !options.acknowledgeHosts) {
      throw new IpcError("host_ack_required", "approve where the API key would be sent first");
    }
    const opened = await this.upgrade({ workspace, approved });
    await this.deps.store.trust(workspace, { configSha256: file.sha256, approvedHosts: approved });
    return opened;
  }

  async forget(path: string): Promise<OpenedWorkspace | null> {
    const listed = await this.deps.request("workspace.list", {});
    const open = listed.workspaces.find((w) => w.workspace === path);
    if (open?.active_turn) {
      throw new IpcError("turn_running", "stop the running turn before changing trust");
    }
    await this.deps.store.forget(path);
    if (!open) return null;
    await this.deps.request("workspace.close", { workspace: path });
    return this.open(path);
  }

  private openRestricted(path: string) {
    return this.deps
      .request("workspace.open", { path, trust_workspace_config: false })
      .then((result) => {
        this.deps.opened(result.workspace, true);
        return result;
      });
  }

  private async destinations(workspace: string): Promise<Destination[]> {
    const { providers } = await this.deps.request("provider.list", { workspace });
    return providers;
  }

  private async turnRunning(workspace: string): Promise<boolean> {
    const { workspaces } = await this.deps.request("workspace.list", {});
    return workspaces.some((w) => w.workspace === workspace && w.active_turn !== null);
  }

  /** Reopens with the config loaded, then checks where the key would go; a mismatch closes it. */
  private async upgrade(options: {
    workspace: string;
    approved: string[];
  }): Promise<OpenedWorkspace> {
    const { workspace, approved } = options;
    if (await this.turnRunning(workspace)) {
      throw new IpcError("turn_running", "stop the running turn before changing trust");
    }
    const baseline = await this.destinations(workspace);
    await this.deps.request("workspace.close", { workspace });
    try {
      await this.deps.request("workspace.open", { path: workspace, trust_workspace_config: true });
    } catch (error) {
      // The config did not load; the folder stays usable, restricted.
      await this.deps.request("workspace.open", { path: workspace, trust_workspace_config: false });
      throw new IpcError("config_invalid", "the workspace config failed to load", { cause: error });
    }
    const verdict = checkDestinations({
      baseline,
      actual: await this.destinations(workspace),
      approved,
    });
    if (!verdict.ok) {
      await this.deps.request("workspace.close", { workspace });
      this.deps.opened(workspace, false);
      throw new IpcError("blocked_config", BLOCKED);
    }
    return { workspace, trust: "trusted" };
  }
}
