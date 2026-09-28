// Negative controls for how a program's run is taken (ADR 0088): a run that
// failed must not pass for one that didn't.

import { expect, test } from "bun:test";
import { writeFileSync } from "node:fs";
import { join } from "node:path";

import { agree, execute, type Run } from "./programs";
import { fixture } from "./support";

const clean: Run = { stdout: "", stderr: "", bytes: { stdout: Buffer.alloc(0), stderr: Buffer.alloc(0) }, outcome: { value: null } };
const script = (code: string) => [process.execPath, "-e", code];

test("a run that fails after writing its outcome failed", () => {
  const file = join(fixture("programs-exit"), "outcome.json");
  const run = execute(script(`require("fs").writeFileSync(${JSON.stringify(file)}, '{"value":null}'); process.exit(7)`), file);
  expect(run.outcome).toBe('exited 7 after it ended {"value":null}');
  expect(agree(run, clean)).toBe(false);
});

test("an outcome left from an earlier run isn't this run's", () => {
  const file = join(fixture("programs-stale"), "outcome.json");
  writeFileSync(file, '{"value":null}');
  const run = execute(script("0"), file);
  expect(run.outcome).toBe("exited 0 without an outcome");
  expect(agree(run, clean)).toBe(false);
});

test("a run that exits 0 with its outcome is taken at its word", () => {
  const file = join(fixture("programs-ok"), "outcome.json");
  const run = execute(script(`require("fs").writeFileSync(${JSON.stringify(file)}, '{"value":null}')`), file);
  expect(agree(run, clean)).toBe(true);
});

// What a run printed is compared byte for byte: a byte that isn't UTF-8
// isn't the U+FFFD it reads as. Found in review: they compared equal.
test("what a run printed is compared byte for byte", () => {
  const file = join(fixture("programs-bytes"), "outcome.json");
  const printing = (hex: string) =>
    execute(script(`process.stdout.write(Buffer.from("${hex}", "hex")); require("fs").writeFileSync(${JSON.stringify(file)}, '{"value":null}')`), file);
  expect(agree(printing("ff"), printing("efbfbd"))).toBe(false);
  expect(agree(printing("efbfbd"), printing("efbfbd"))).toBe(true);
});
