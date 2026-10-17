// @vitest-environment node
import { describe, expect, test, vi } from "vitest";
import { QuitGuard } from "../../../src/main/quit";

function setup(options: { running: boolean; confirmed?: boolean; shutdownMs?: number }) {
  const log: string[] = [];
  let finish: () => void = () => undefined;
  const ports = {
    turnRunning: () => options.running,
    confirm: vi.fn(async () => {
      log.push("confirm");
      return options.confirmed ?? true;
    }),
    shutdown: vi.fn(async (force: boolean) => {
      log.push(force ? "force" : "shutdown");
      if (!force && options.shutdownMs) await new Promise<void>((r) => (finish = r));
    }),
    quit: vi.fn(() => log.push("quit")),
  };
  return { guard: new QuitGuard(ports), ports, log, release: () => finish() };
}

const settle = () => new Promise((r) => setTimeout(r, 0));

describe("QuitGuard", () => {
  test("with nothing running it stops the sidecar and then quits, without asking", async () => {
    const t = setup({ running: false });
    expect(t.guard.handle()).toBe(false);
    await settle();
    expect(t.log).toEqual(["shutdown", "quit"]);
    expect(t.ports.confirm).not.toHaveBeenCalled();
    expect(t.guard.handle()).toBe(true);
  });

  test("a running turn asks first, and confirming goes ahead", async () => {
    const t = setup({ running: true });
    t.guard.handle();
    await settle();
    expect(t.log).toEqual(["confirm", "shutdown", "quit"]);
  });

  test("declining changes nothing and quitting can be tried again", async () => {
    const t = setup({ running: true, confirmed: false });
    expect(t.guard.handle()).toBe(false);
    await settle();
    expect(t.log).toEqual(["confirm"]);
    t.guard.handle();
    await settle();
    expect(t.log).toEqual(["confirm", "confirm"]);
  });

  test("a second quit while the sidecar is stopping takes the kill path", async () => {
    const t = setup({ running: false, shutdownMs: 1 });
    t.guard.handle();
    await settle();
    expect(t.guard.handle()).toBe(false);
    expect(t.log).toEqual(["shutdown", "force"]);
    t.release();
    await settle();
    expect(t.log.at(-1)).toBe("quit");
  });

  test("the app still quits when the shutdown fails", async () => {
    const t = setup({ running: false });
    t.ports.shutdown.mockRejectedValueOnce(new Error("boom"));
    t.guard.handle();
    await settle();
    expect(t.ports.quit).toHaveBeenCalled();
  });
});
