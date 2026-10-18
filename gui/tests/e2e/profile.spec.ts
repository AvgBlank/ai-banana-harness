import { readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import type { Launched } from "./fixtures";
import { expect, test } from "./fixtures";

async function openProfile(app: Launched) {
  await app.page.getByRole("button", { name: /Settings/ }).click();
  const field = app.page.getByLabel("Display name");
  await expect(field).toBeVisible();
  return field;
}

test("a display name persists across a relaunch, is private, and holds no secret", async ({
  launch,
  stub,
}) => {
  const first = await launch();
  const field = await openProfile(first);
  await expect(field).toHaveValue("");
  await field.fill("  Ada Lovelace  ");
  await field.press("Enter");
  await expect(first.page.getByRole("img", { name: "Profile: Ada Lovelace" })).toBeVisible();

  const file = join(first.userData, "preferences.json");
  await expect.poll(() => readFileSync(file, "utf8")).toContain("Ada Lovelace");
  expect(statSync(file).mode & 0o777).toBe(0o600);
  const stored = readFileSync(file, "utf8");
  expect(JSON.parse(stored).profile).toEqual({ displayName: "Ada Lovelace" });
  expect(stored).not.toMatch(/key|token|secret/i);
  expect(stub.env.AI_API_KEY).toBeTruthy();
  expect(stored).not.toContain(stub.env.AI_API_KEY as string);
  await first.app.close();

  const second = await launch();
  const again = await openProfile(second);
  await expect(again).toHaveValue("Ada Lovelace");
  await expect(second.page.getByRole("img", { name: "Profile: Ada Lovelace" })).toBeVisible();
});

test("an invalid name shows an error and is not saved; Clear returns to no profile", async ({
  launch,
}) => {
  const app = await launch();
  const field = await openProfile(app);
  await field.fill("Grace");
  await field.press("Enter");
  await expect(app.page.getByRole("img", { name: "Profile: Grace" })).toBeVisible();

  await field.fill("x".repeat(61));
  await field.press("Enter");
  await expect(app.page.getByRole("alert")).toContainText("60");
  await expect(app.page.getByRole("img", { name: "Profile: Grace" })).toBeVisible();
  const file = join(app.userData, "preferences.json");
  expect(JSON.parse(readFileSync(file, "utf8")).profile).toEqual({ displayName: "Grace" });

  await app.page.getByRole("button", { name: "Clear" }).click();
  await expect(field).toHaveValue("");
  await expect(app.page.getByRole("img", { name: /^Profile:/ })).toBeHidden();
  await expect.poll(() => JSON.parse(readFileSync(file, "utf8")).profile).toBeNull();
});
