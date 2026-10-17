import { execSync } from "node:child_process";
import type { Page } from "@playwright/test";
import type { Launched, Stub } from "../fixtures";
import { expect } from "../fixtures";

/** Opens the stub's workspace the way a user does: Open folder, with main's dialog answered. */
export async function openFolder(app: Launched, stub: Stub): Promise<void> {
  await app.app.evaluate(({ dialog }, folder) => {
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [folder] });
  }, stub.workspace);
  const open = app.page.getByRole("button", { name: "Open folder" });
  await expect(open).toBeEnabled({ timeout: 20_000 });
  const name = stub.workspace.split("/").at(-1) as string;
  await expect(async () => {
    await open.click();
    await expect(app.page.getByRole("heading", { level: 1 })).toContainText(name, {
      timeout: 2_000,
    });
  }).toPass({ timeout: 20_000 });
}

export async function sendPrompt(page: Page, text: string): Promise<void> {
  await page.getByLabel("Task").fill(text);
  await page.getByRole("button", { name: "Send" }).click();
}

/** The sidecars started by the process `pid`; Electron's helper processes are children too. */
export function sidecarsOf(pid: number): number[] {
  try {
    return execSync(`pgrep -P ${pid} -f "serve --stdio"`, { encoding: "utf8" })
      .split("\n")
      .filter(Boolean)
      .map(Number);
  } catch {
    return [];
  }
}

export function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/** Reopens the session in the sidebar after a reload; the page forgets which one was on screen. */
export async function reopenSession(page: Page, title: RegExp): Promise<void> {
  await page
    .getByRole("complementary", { name: "Workspaces and sessions" })
    .getByRole("button", { name: title })
    .first()
    .click();
}
