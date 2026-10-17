/** The workspaces main has seen from a trustworthy source; the renderer cannot add to this. */
export class KnownWorkspaces {
  private readonly paths = new Set<string>();

  add(path: string): void {
    this.paths.add(path);
  }

  has(path: string): boolean {
    return this.paths.has(path);
  }

  /** A session list names the workspaces the database already holds. */
  learnFromSessions(sessions: ReadonlyArray<{ workspace: string }>): void {
    for (const session of sessions) this.paths.add(session.workspace);
  }
}
