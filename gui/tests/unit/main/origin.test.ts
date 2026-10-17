// @vitest-environment node
import { mkdirSync, mkdtempSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, test, vi } from "vitest";

vi.mock("electron", () => ({ net: {}, protocol: {} }));

const { CONTENT_SECURITY_POLICY, devContentSecurityPolicy, isAllowedNavigation, resolveAppPath } =
  await import("../../../src/main/origin");

describe("resolveAppPath", () => {
  const root = mkdtempSync(join(tmpdir(), "bh-origin-"));
  mkdirSync(join(root, "assets"));
  writeFileSync(join(root, "index.html"), "x");
  writeFileSync(join(root, "assets", "a.js"), "x");
  const outside = mkdtempSync(join(tmpdir(), "bh-outside-"));
  writeFileSync(join(outside, "secret.txt"), "x");
  symlinkSync(outside, join(root, "link"));
  const real = (name: string) => join(root, name);

  test("the root and normal files resolve inside the root", () => {
    expect(resolveAppPath(root, "/")).toContain("index.html");
    expect(resolveAppPath(root, "/index.html")).toContain("index.html");
    expect(resolveAppPath(root, "/assets/a.js")).toContain("a.js");
    expect(resolveAppPath(root, "/assets/./a.js")).toContain("a.js");
    expect(real("index.html")).toBeTruthy();
  });

  test("traversal, encoded traversal, backslashes, NUL and symlinks out of the root are refused", () => {
    for (const path of [
      "/../secret",
      "/assets/../../etc/passwd",
      "/%2e%2e/etc/passwd",
      "/assets/%2e%2e/%2e%2e/etc/passwd",
      "/..%2f..%2fetc/passwd",
      "/assets\\..\\..\\x",
      "/index.html%00.png",
      "/link/secret.txt",
      "/%zz",
    ]) {
      expect(resolveAppPath(root, path), path).toBeNull();
    }
  });

  test("a file that does not exist resolves to nothing", () => {
    expect(resolveAppPath(root, "/missing.js")).toBeNull();
  });
});

describe("navigation", () => {
  test("only the app origin (and the dev server in development) is allowed", () => {
    expect(isAllowedNavigation("app://banana/index.html", undefined)).toBe(true);
    expect(isAllowedNavigation("app://banana", undefined)).toBe(true);
    for (const url of [
      "https://example.com/",
      "file:///etc/hosts",
      "app://evil/index.html",
      "app://banana.evil.com/",
      "javascript:alert(1)",
      "data:text/html,x",
      "http://localhost:5173/",
    ]) {
      expect(isAllowedNavigation(url, undefined), url).toBe(false);
    }
    expect(isAllowedNavigation("http://localhost:5173/x", "http://localhost:5173")).toBe(true);
    expect(isAllowedNavigation("http://localhost:5173.evil.com/", "http://localhost:5173")).toBe(
      false,
    );
  });
});

describe("content security policy", () => {
  test("production policy is strict", () => {
    expect(CONTENT_SECURITY_POLICY).toContain("default-src 'none'");
    expect(CONTENT_SECURITY_POLICY).toContain("script-src 'self'");
    expect(CONTENT_SECURITY_POLICY).not.toMatch(/script-src[^;]*unsafe/);
    expect(CONTENT_SECURITY_POLICY).toContain("connect-src 'self'");
    expect(CONTENT_SECURITY_POLICY).toContain("frame-ancestors 'none'");
    expect(CONTENT_SECURITY_POLICY).toContain("object-src 'none'");
  });

  test("only the dev policy is loosened, and only for the dev server", () => {
    const dev = devContentSecurityPolicy("http://localhost:5173");
    expect(dev).toContain("script-src 'self' 'unsafe-inline'");
    expect(dev).toContain("connect-src 'self' http://localhost:5173 ws://localhost:5173");
    expect(CONTENT_SECURITY_POLICY).not.toContain("localhost");
  });
});
