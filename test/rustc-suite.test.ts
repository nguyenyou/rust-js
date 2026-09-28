// The rustc suite's own logic (ADR 0089): which tests fit a corpus case,
// and the ratchet that keeps the known failures from growing or going stale.

import { expect, test } from "bun:test";

import { homedir } from "node:os";

import { failureKind, firstError, ratchet, scope, surprises, validate, type Result, type Shard } from "../scripts/rustc-suite";

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
  expect(ratchet(results.slice(0, 2), new Map([["b.rs", "x"]]))).toEqual({ regressions: [], fixed: [], worse: [] });
});

test("a failure is rejected, crashed or wrong, and one listed as rejected may not get worse", () => {
  expect(failureKind("error: rust-js does not support statics yet")).toBe("rejected");
  expect(failureKind("error: rust-js: generated trait implementation name `x` collides")).toBe("rejected");
  expect(failureKind("error: internal compiler error: ~/.rustup/x.rs:1:2: oops")).toBe("crashed");
  expect(failureKind("thread 'rustc' panicked at /rustc-dev/x.rs:1:2:")).toBe("crashed");
  expect(failureKind('bun: ended {"panic":"x"}')).toBe("wrong");
  expect(failureKind("node: different stdout")).toBe("wrong");
  const listed = new Map([
    ["a.rs", "error: rust-js does not support statics yet"],
    ["b.rs", "error: rust-js does not support statics yet"],
    ["c.rs", "error: rust-js does not support statics yet"],
    ["d.rs", "bun: different stdout"],
  ]);
  const now: Result[] = [
    { test: "a.rs", status: "fail", reason: "error: rust-js does not support unions yet" },
    { test: "b.rs", status: "fail", reason: "thread 'rustc' panicked at x.rs:1:2:" },
    { test: "c.rs", status: "fail", reason: "bun: different stdout" },
    { test: "d.rs", status: "fail", reason: "error: rust-js does not support statics yet" },
  ];
  // Another rejection is no worse; a crash or a wrong answer is; a wrong
  // answer turned into a rejection is better.
  expect(ratchet(now, listed).worse.map((r) => r.test)).toEqual(["b.rs", "c.rs"]);
});

// A run of some tests, as the workflow's `tests` makes, is checked as a
// whole run is: a listed rejection that now crashes isn't as listed either.
// Found in review.
test("a run of some tests says which aren't as listed, a worse failure too", () => {
  const listed = new Map([
    ["a.rs", "error: rust-js does not support statics yet"],
    ["b.rs", "error: rust-js does not support statics yet"],
    ["c.rs", "error: rust-js does not support statics yet"],
  ]);
  const now: Result[] = [
    { test: "a.rs", status: "fail", reason: "error: rust-js does not support statics yet" },
    { test: "b.rs", status: "fail", reason: "thread 'rustc' panicked at x.rs:1:2:" },
    { test: "c.rs", status: "pass" },
    { test: "d.rs", status: "fail", reason: "error: rust-js does not support unions yet" },
    { test: "e.rs", status: "pass" },
    { test: "f.rs", status: "skip", reason: "has revisions" },
  ];
  expect([...surprises(now, listed)].sort()).toEqual(["b.rs", "c.rs", "d.rs"]);
});

// The shards of a run are one whole run, or it isn't checked or blessed.
// Found in review: an empty list of results passed, with no tests run.
test("shards are checked as one whole run before their results are", () => {
  const inventory = ["a.rs", "b.rs", "c.rs", "d.rs"];
  const shard = (i: number, more: Partial<Shard> = {}): Shard => {
    const expected = inventory.filter((_, k) => k % 2 === i - 1);
    return {
      shard: i,
      of: 2,
      compiler: "c1",
      toolchain: "t1",
      source: "s1",
      inventory,
      expected,
      results: expected.map((test): Result => ({ test, status: "pass" })),
      ...more,
    };
  };
  const known = new Map([["b.rs", "x"]]);
  expect(validate([shard(1), shard(2)], known, "s1")).toEqual([]);
  expect(validate([], known, "s1")).toEqual(["there are no shards"]);
  expect(validate([[] as unknown as Shard], known, "s1")).toEqual(["1 of the 1 files aren't a shard's record"]);
  expect(validate([shard(1)], known, "s1")).toEqual(["shards missing, of 2: 2", "tests with no result: 2, as b.rs, d.rs"]);
  expect(validate([shard(1), shard(1), shard(2)], known, "s1")).toEqual(["shards there more than once, of 2: 1", "tests with more than one result: 2, as a.rs, c.rs"]);
  expect(validate([shard(1), shard(2, { compiler: "c2" })], known, "s1")).toEqual(["the shards ran with different compilers: c1, c2"]);
  expect(validate([shard(1), shard(2)], known, "s2")).toEqual(["the shards ran source s1, and this is s2"]);
  expect(validate([shard(1), shard(2, { results: [] })], known, "s1")).toEqual(["tests with no result: 2, as b.rs, d.rs"]);
  expect(validate([shard(1), shard(2, { expected: ["b.rs"], results: [{ test: "b.rs", status: "pass" }] })], known, "s1")).toEqual([
    "shard 2 was to run other tests than its share",
    "tests with no result: 1, as d.rs",
  ]);
  expect(validate([shard(1), shard(2)], new Map([["gone.rs", "x"]]), "s1")).toEqual(["known failures that aren't tests: 1, as gone.rs"]);
  expect(validate([shard(1, { inventory: [], expected: [], results: [] }), shard(2, { inventory: [], expected: [], results: [] })], new Map(), "s1")).toEqual(["there were no tests to run"]);
});
