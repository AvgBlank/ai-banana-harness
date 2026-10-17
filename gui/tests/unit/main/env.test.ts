// @vitest-environment node
import { expect, test } from "vitest";
import { redact, sidecarEnv } from "../../../src/main/sidecar/env";

test("only allowlisted variables, the resolved PATH and the key reach the sidecar", () => {
  const source = {
    HOME: "/home/u",
    USER: "u",
    LOGNAME: "u",
    SHELL: "/bin/zsh",
    TMPDIR: "/tmp",
    LANG: "en_US.UTF-8",
    LC_ALL: "C",
    LC_CTYPE: "UTF-8",
    XDG_CONFIG_HOME: "/c",
    XDG_DATA_HOME: "/d",
    XDG_CACHE_HOME: "/cache",
    HTTP_PROXY: "http://p",
    https_proxy: "http://p",
    ALL_PROXY: "http://p",
    no_proxy: "localhost",
    SSL_CERT_FILE: "/f",
    SSL_CERT_DIR: "/d",
    PATH: "/usr/bin",
    AI_API_KEY: "from-the-process-env",
    NODE_OPTIONS: "--inspect",
    ELECTRON_RUN_AS_NODE: "1",
    ELECTRON_ENABLE_LOGGING: "1",
    SECRET_TOKEN: "x",
  };
  const env = sidecarEnv(source, { path: "/opt/bin:/usr/bin", apiKey: "resolved-key" });
  expect(Object.keys(env).sort()).toEqual(
    [
      "ALL_PROXY",
      "AI_API_KEY",
      "HOME",
      "HTTP_PROXY",
      "LANG",
      "LC_ALL",
      "LC_CTYPE",
      "LOGNAME",
      "PATH",
      "SHELL",
      "SSL_CERT_DIR",
      "SSL_CERT_FILE",
      "TMPDIR",
      "USER",
      "XDG_CONFIG_HOME",
      "XDG_DATA_HOME",
      "https_proxy",
      "no_proxy",
    ].sort(),
  );
  expect(env.PATH).toBe("/opt/bin:/usr/bin");
  expect(env.AI_API_KEY).toBe("resolved-key");
});

test("the key comes only from the resolver, not from the inherited environment", () => {
  expect(sidecarEnv({ AI_API_KEY: "inherited", PATH: "/p" }, {})).toEqual({ PATH: "/p" });
});

test("redact replaces every occurrence and leaves text alone without a key", () => {
  expect(redact("a KEY b KEY", "KEY")).toBe("a [REDACTED] b [REDACTED]");
  expect(redact("nothing", undefined)).toBe("nothing");
  expect(redact("nothing", "")).toBe("nothing");
});
