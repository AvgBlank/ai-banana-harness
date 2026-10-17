// @vitest-environment node

import { chmodSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "vitest";
import { parseProbeOutput, probeLoginShell } from "../../../src/main/key/probe";
import { tempDir } from "./tmp";

const BEGIN = "__BH_PROBE_BEGIN__";
const END = "__BH_PROBE_END__";

/** A stand-in for `$SHELL`: runs `body` (a /bin/sh script) whatever arguments it is given. */
function stubShell(body: string): string {
  const file = join(tempDir("bh-shell-"), "shell");
  writeFileSync(file, `#!/bin/sh\n${body}\n`);
  chmodSync(file, 0o755);
  return file;
}

test("the wanted variables between the sentinels parse, and rc-file noise around them is ignored", () => {
  const output = [
    "Last login: Sat Oct 17",
    "AI_API_KEY=sk-noise-before-the-block",
    BEGIN,
    "HOME=/Users/me",
    "PATH=/opt/homebrew/bin:/usr/bin",
    "AI_API_KEY=sk-from-the-shell",
    "https_proxy=http://proxy:3128",
    "NO_PROXY=localhost",
    "SECRET=ignored",
    "BROKEN LINE WITHOUT EQUALS",
    END,
    "logout noise AI_API_KEY=sk-noise-after",
  ].join("\n");
  expect(parseProbeOutput(output)).toEqual({
    PATH: "/opt/homebrew/bin:/usr/bin",
    AI_API_KEY: "sk-from-the-shell",
    https_proxy: "http://proxy:3128",
    NO_PROXY: "localhost",
  });
});

test("no complete block, or an empty one, is not an error", () => {
  expect(parseProbeOutput("")).toBeNull();
  expect(parseProbeOutput(`${BEGIN}\nPATH=/x`)).toBeNull();
  expect(parseProbeOutput(`${END}\n${BEGIN}`)).toBeNull();
  expect(parseProbeOutput(`${BEGIN}\n${END}`)).toEqual({});
  expect(parseProbeOutput(`${BEGIN}\nAI_API_KEY=\n${END}`)).toEqual({});
});

test("a stub login shell with noisy rc output yields its variables", async () => {
  const shell = stubShell(
    `echo "welcome to my shell"; echo "AI_API_KEY=decoy"; printf '%s\\n' ${BEGIN}; echo PATH=/probe/bin:/usr/bin; echo AI_API_KEY=sk-probe; printf '%s\\n' ${END}; echo "bye"`,
  );
  expect(await probeLoginShell(shell, 5000)).toEqual({
    PATH: "/probe/bin:/usr/bin",
    AI_API_KEY: "sk-probe",
  });
});

test("the shell is started as an interactive login shell with the script as a command", async () => {
  const args = join(tempDir("bh-args-"), "args.txt");
  const shell = stubShell(`printf '%s\\n' "$1" > ${args}; printf '%s\\n' ${BEGIN} ${END}`);
  await probeLoginShell(shell, 5000);
  const { readFileSync } = await import("node:fs");
  expect(readFileSync(args, "utf8").trim()).toBe("-ilc");
});

test("a hanging shell is killed with its process group when the timeout passes, and falls through", async () => {
  const marker = join(tempDir("bh-hang-"), "child.pid");
  const shell = stubShell(`sleep 30 & echo $! > ${marker}; sleep 30`);
  const started = Date.now();
  expect(await probeLoginShell(shell, 1500)).toBeNull();
  expect(Date.now() - started).toBeLessThan(4000);
  const { readFileSync } = await import("node:fs");
  const pid = Number(readFileSync(marker, "utf8").trim());
  await new Promise((resolve) => setTimeout(resolve, 200));
  expect(() => process.kill(pid, 0), "the background child is still alive").toThrow();
});

test("a missing, failing or empty shell is null, not an error", async () => {
  expect(await probeLoginShell(undefined)).toBeNull();
  expect(await probeLoginShell("/no/such/shell")).toBeNull();
  expect(await probeLoginShell(stubShell("exit 3"))).toBeNull();
  expect(await probeLoginShell(stubShell(`printf '%s\\n' ${BEGIN} ${END}`))).toEqual({});
});
