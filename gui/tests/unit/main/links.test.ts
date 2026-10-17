// @vitest-environment node
import { describe, expect, test, vi } from "vitest";
import { ExternalLinks, parseExternalUrl } from "../../../src/main/external/links";

describe("parseExternalUrl", () => {
  test("https anywhere and http on loopback are accepted", () => {
    for (const url of [
      "https://example.com/a?b=c#d",
      "http://localhost:8080/cb",
      "http://127.0.0.1/cb",
      "http://[::1]:3000/cb",
    ]) {
      expect(parseExternalUrl(url).toString(), url).toBeTruthy();
    }
  });

  test("other schemes, plain http elsewhere, credentials and junk are refused", () => {
    for (const url of [
      "http://example.com/",
      "http://localhost.evil.com/",
      "file:///etc/passwd",
      "javascript:alert(1)",
      "data:text/html,x",
      "ftp://example.com/",
      "app://banana/",
      "vscode://open",
      "https://user:pw@example.com/",
      "https://user@example.com/",
      "not a url",
      "",
    ]) {
      expect(() => parseExternalUrl(url), url).toThrow();
    }
  });
});

describe("ExternalLinks", () => {
  test("a refused link never reaches the confirm dialog or the browser", async () => {
    const ports = { confirm: vi.fn(), open: vi.fn() };
    await expect(new ExternalLinks(ports).open("file:///etc/hosts")).rejects.toMatchObject({
      code: "invalid_url",
    });
    expect(ports.confirm).not.toHaveBeenCalled();
    expect(ports.open).not.toHaveBeenCalled();
  });

  test("the user sees the full URL and a cancel opens nothing", async () => {
    const ports = {
      confirm: vi.fn(async (_url: URL) => ({ open: false, trustHost: false })),
      open: vi.fn(),
    };
    await new ExternalLinks(ports).open("https://example.com/path?q=1");
    expect(ports.confirm.mock.calls[0]?.[0].toString()).toBe("https://example.com/path?q=1");
    expect(ports.open).not.toHaveBeenCalled();
  });

  test("a confirmed link opens, and a trusted host is not asked about again this session", async () => {
    const ports = {
      confirm: vi.fn(async (_url: URL) => ({ open: true, trustHost: true })),
      open: vi.fn(async () => undefined),
    };
    const links = new ExternalLinks(ports);
    await links.open("https://example.com/one");
    await links.open("https://example.com/two");
    await links.open("https://other.example.org/");
    expect(ports.confirm).toHaveBeenCalledTimes(2);
    expect(ports.open).toHaveBeenCalledTimes(3);
  });
});
