import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { root } from "./support";

const channel = (Bun.TOML.parse(readFileSync(join(root, "rust-toolchain.toml"), "utf8")) as { toolchain: { channel: string } }).toolchain.channel;
const run = (...args: string[]) => Bun.spawnSync([process.execPath, "scripts/toolchain.ts", ...args], {
  cwd: root, stdout: "pipe", stderr: "pipe",
});

test("toolchain helper resolves the root pin and installed compiler source", () => {
  const pin = run("channel");
  expect(pin.exitCode).toBe(0);
  expect(pin.stdout.toString().trim()).toBe(channel);
  const version = Bun.spawnSync(["rustc", `+${channel}`, "-vV"], { cwd: root });
  expect(version.exitCode).toBe(0);
  const commit = run("commit");
  expect(commit.exitCode).toBe(0);
  const hash = version.stdout.toString().match(/^commit-hash: (.+)$/m)?.[1];
  if (!hash) throw new Error("rustc did not report a commit hash");
  expect(commit.stdout.toString().trim()).toBe(hash);
});

test("toolchain helper rejects a checkout at a different commit", () => {
  const result = run("check-source", root);
  expect(result.exitCode).toBe(1);
  expect(result.stderr.toString()).toContain("rustc source mismatch:");
  expect(result.stderr.toString()).toContain("Use a checkout at the pinned commit");
});

test("toolchain helper rejects missing and unexpected arguments", () => {
  for (const args of [[], ["check-source"], ["channel", "extra"], ["unknown"]]) {
    expect(run(...args).exitCode).toBe(1);
  }
});
