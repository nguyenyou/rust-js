// A distribution is qualified only as its files are (ADR 0094): each one
// SHA256SUMS lists, with the hash it has, and each artifact the one
// distribution.json names. Installing it and running the suite is
// scripts/qualify.ts's.

import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { checksums } from "../scripts/qualify";
import { fixture } from "./support";

const sha256 = (text: string) => createHash("sha256").update(text).digest("hex");

function distribution(): string {
  const dir = fixture("qualify-sums");
  writeFileSync(join(dir, "a.tgz"), "a");
  writeFileSync(join(dir, "b.tgz"), "b");
  const manifest = JSON.stringify({ artifacts: [{ file: "a.tgz", sha256: sha256("a") }, { file: "b.tgz", sha256: sha256("b") }] });
  writeFileSync(join(dir, "distribution.json"), manifest);
  writeFileSync(join(dir, "SHA256SUMS"), `${sha256("a")}  a.tgz\n${sha256("b")}  b.tgz\n${sha256(manifest)}  distribution.json\n`);
  return dir;
}

test("a distribution's files are what its checksums say, or it isn't qualified", () => {
  expect(checksums(distribution())).toEqual([]);
  const damaged = distribution();
  writeFileSync(join(damaged, "b.tgz"), "not b");
  expect(checksums(damaged)).toEqual(["b.tgz isn't what SHA256SUMS says", "b.tgz isn't what distribution.json says"]);
  const missing = distribution();
  rmSync(join(missing, "a.tgz"));
  expect(checksums(missing)).toEqual(["a.tgz is missing"]);
  const extra = distribution();
  writeFileSync(join(extra, "SHA256SUMS"), `${sha256("c")}  c.tgz\n`, { flag: "a" });
  expect(checksums(extra)).toEqual(["SHA256SUMS lists c.tgz, which the distribution doesn't"]);
});
