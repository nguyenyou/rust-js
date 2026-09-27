// The rustc suite's own logic (ADR 0089): which tests fit a corpus case,
// and the ratchet that keeps the known failures from growing or going stale.

import { expect, test } from "bun:test";

import { homedir } from "node:os";

import { firstError, ratchet, scope, type Result } from "../scripts/rustc-suite";

test("a test is in scope unless a directive says it needs what a case can't have", () => {
  expect(scope("//@ run-pass\nfn main() {}\n")).toEqual({ edition: "2015" });
  expect(scope("//@ run-pass\n//@ edition: 2021\nfn main() {}\n")).toEqual({ edition: "2021" });
  expect(scope("//@ run-pass\n//@ edition:2018\nfn main() {}\n")).toEqual({ edition: "2018" });
  expect(scope("//@ run-pass\n//@ needs-unwind\nfn main() {}\n")).toEqual({ edition: "2015" });
  expect(scope("//@ run-pass\n//@ aux-build: helper.rs\nfn main() {}\n")).toEqual({ skip: "needs another crate" });
  expect(scope("//@ run-pass\n//@ revisions: a b\nfn main() {}\n")).toEqual({ skip: "has revisions" });
  expect(scope("//@ run-pass\n//@ compile-flags: -O\nfn main() {}\n")).toEqual({ skip: "needs flags or an environment of its own" });
  expect(scope("//@ run-pass\n//@ needs-threads\nfn main() {}\n")).toEqual({ skip: "needs a capability of its own" });
  expect(scope("//@ run-pass\n//@ only-x86_64\nfn main() {}\n")).toEqual({ skip: "is for some targets only" });
  expect(scope("//@ run-pass\n//@ only-x86_64 (a comment)\nfn main() {}\n")).toEqual({ skip: "is for some targets only" });
  expect(scope("//@ run-pass\n//@compile-flags: -O\nfn main() {}\n")).toEqual({ skip: "needs flags or an environment of its own" });
  expect(scope("//@ run-pass\n//@ ignore-wasm32\nfn main() {}\n")).toEqual({ skip: "doesn't apply to wasm, whose integers rust-js has" });
  expect(scope("//@ run-pass\nmod helper;\nfn main() {}\n")).toEqual({ skip: "has modules in other files" });
  expect(scope('//@ run-pass\nfn main() { let s = include_str!("data.txt"); }\n')).toEqual({ skip: "reads files beside it" });
  expect(scope("//@ run-pass\n#![feature(staged_api)]\nfn main() {}\n")).toEqual({ skip: "is the standard library's own API" });
  expect(scope("//@ run-pass\nfn main() -> Result<(), ()> { Ok(()) }\n")).toEqual({ skip: "has no `fn main() {`" });
});

test("a failure's reason names no path of this machine's", () => {
  const stderr = `warning: x\nerror: rust-js does not support \`{closure@${homedir()}/w/target/rustc-suite/case-Ab12Cd/t.rs:3:9}\` yet\n`;
  expect(firstError(stderr)).toBe("error: rust-js does not support `{closure@<case>/t.rs:3:9}` yet");
  expect(firstError(`error: internal compiler error: ${homedir()}/.rustup/toolchains/nightly-2026-03-25-x86_64-unknown-linux-gnu/lib/x.rs:1:2: oops`)).toBe(
    "error: internal compiler error: ~/.rustup/toolchains/<toolchain>/lib/x.rs:1:2: oops",
  );
  // A thread's number is this run's.
  expect(firstError("thread 'rustc' (17338662) panicked at /rustc-dev/abc/compiler/x.rs:3:9:")).toBe(
    "thread 'rustc' panicked at /rustc-dev/abc/compiler/x.rs:3:9:",
  );
});

test("the ratchet reports a new failure and a listed test that passes", () => {
  const results: Result[] = [
    { test: "a.rs", status: "pass" },
    { test: "b.rs", status: "fail", reason: "x" },
    { test: "c.rs", status: "fail", reason: "y" },
    { test: "d.rs", status: "pass" },
    { test: "e.rs", status: "skip", reason: "has revisions" },
  ];
  const known = new Map([["b.rs", "x"], ["d.rs", "z"], ["e.rs", "w"]]);
  const { regressions, fixed } = ratchet(results, known);
  expect(regressions.map((r) => r.test)).toEqual(["c.rs"]);
  expect(fixed.map((r) => r.test)).toEqual(["d.rs"]);
  // Nothing new: nothing to say.
  expect(ratchet(results.slice(0, 2), new Map([["b.rs", "x"]]))).toEqual({ regressions: [], fixed: [] });
});
