export interface QuitPorts {
  /** A turn is running somewhere. */
  turnRunning: () => boolean;
  /** Asks whether to quit and cancel it. */
  confirm: () => Promise<boolean>;
  /** Stops the sidecar; `force` skips the polite steps. */
  shutdown: (force: boolean) => Promise<void>;
  quit: () => void;
}

/**
 * What Quit does: confirm when a turn is running, then stop the sidecar before the app exits. A
 * second Quit while that runs takes the kill path.
 */
export class QuitGuard {
  private stopping = false;
  private stopped = false;

  constructor(private readonly ports: QuitPorts) {}

  /** Returns `true` when the quit may go ahead as it is, `false` when it was taken over. */
  handle(): boolean {
    if (this.stopped) return true;
    if (this.stopping) {
      void this.ports.shutdown(true);
      return false;
    }
    this.stopping = true;
    void this.run();
    return false;
  }

  private async run(): Promise<void> {
    if (this.ports.turnRunning() && !(await this.ports.confirm())) {
      this.stopping = false;
      return;
    }
    try {
      await this.ports.shutdown(false);
    } catch {
      // The app quits regardless; the sidecar also exits on its own when stdin closes.
    } finally {
      this.stopped = true;
      this.ports.quit();
    }
  }
}
