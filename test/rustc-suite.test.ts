// The rustc suite's own logic (ADR 0089): which tests fit a corpus case,
// and the ratchet that keeps the known failures from growing or going stale.

import { expect, test } from "bun:test";

import { homedir } from "node:os";

import { failureKind, features, firstError, ratchet, scope, stableFeatures, surprises, unblessable, validate, type Result, type Shard } from "../scripts/rustc-suite";

test("a test is in scope unless a directive says it needs what a case can't have", () => {
  expect(scope("//@ run-pass\nfn main() {}\n")).toEqual({ edition: "2015" });
  expect(scope("//@ run-pass\n//@ edition: 2021\nfn main() {}\n")).toEqual({ edition: "2021" });
  expect(scope("//@ run-pass\n//@ edition:2018\nfn main() {}\n")).toEqual({ edition: "2018" });
  // A range, half-open, is its lowest edition, as compiletest runs it by default.
  expect(scope("//@ run-pass\n//@ edition:2015..2021\nfn main() {}\n")).toEqual({ edition: "2015" });
  expect(scope("//@ run-pass\n//@ edition: 2021..\nfn main() {}\n")).toEqual({ edition: "2021" });
  expect(scope("//@ run-pass\n//@ needs-unwind\nfn main() {}\n")).toEqual({ edition: "2015" });
  expect(scope("//@ run-pass\n//@ aux-build: helper.rs\nfn main() {}\n")).toEqual({ skip: "needs another crate" });
  expect(scope("//@ run-pass\n//@ revisions: a b\nfn main() {}\n")).toEqual({ skip: "has revisions" });
  expect(scope("//@ run-pass\n//@ compile-flags: -O\nfn main() {}\n")).toEqual({ skip: "needs flags or an environment of its own" });
  expect(scope("//@ run-pass\n//@ needs-threads\nfn main() {}\n")).toEqual({ skip: "needs a capability of its own" });
  expect(scope("//@ run-pass\n//@ only-x86_64\nfn main() {}\n")).toEqual({ skip: "is for some targets only" });
  expect(scope("//@ run-pass\n//@ only-x86_64 (a comment)\nfn main() {}\n")).toEqual({ skip: "is for some targets only" });
  expect(scope("//@ run-pass\n//@ ignore-aarch64\nfn main() {}\n")).toEqual({ skip: "is for some targets only" });
  expect(scope("//@ run-pass\n//@ ignore-x86_64\nfn main() {}\n")).toEqual({ skip: "is for some targets only" });
  expect(scope("//@ run-pass\n//@compile-flags: -O\nfn main() {}\n")).toEqual({ skip: "needs flags or an environment of its own" });
  expect(scope("//@ run-pass\n//@ ignore-wasm32\nfn main() {}\n")).toEqual({ skip: "doesn't apply to wasm, whose integers rust-js has" });
  expect(scope("//@ run-pass\nmod helper;\nfn main() {}\n")).toEqual({ skip: "has modules in other files" });
  expect(scope('//@ run-pass\nfn main() { let s = include_str!("data.txt"); }\n')).toEqual({ skip: "reads files beside it" });
  expect(scope("//@ run-pass\n#![feature(staged_api)]\nfn main() {}\n")).toEqual({ skip: "is the standard library's own API" });
  expect(scope("//@ run-pass\nfn main() -> Result<(), ()> { Ok(()) }\n")).toEqual({ skip: "has no `fn main() {`" });
});

// rust-js takes stable Rust (ADR 0109): a test of a feature a stable release
// doesn't have is code no program of rust-js's can be. One whose features a
// stable release has, which the attribute names only from before, is in scope.
test("a test that needs an unstable feature is out of scope, and one of stable features isn't", () => {
  const source = "//@ run-pass\n#![feature(let_chains)]\n#![feature(\n    async_drop, // its own\n    never_type,\n)]\nfn main() {}\n";
  expect(features(source)).toEqual(["let_chains", "async_drop", "never_type"]);
  expect(scope(source, new Set(["let_chains"]))).toEqual({ skip: "needs unstable features: async_drop, never_type" });
  expect(scope(source, new Set(["let_chains", "async_drop", "never_type"]))).toEqual({ edition: "2015" });
  expect(scope("//@ run-pass\n#![allow(unused)]\nfn main() {}\n")).toEqual({ edition: "2015" });
});

// Which features are stable is the pinned rustc's to say: it warns that an
// attribute names one stable since a release, and of an unknown one, errs.
test("the pinned rustc says which features are stable", () => {
  expect(stableFeatures(["let_chains", "async_drop", "iter_zip", "not_a_feature_of_rustc"])).toEqual(new Set(["let_chains", "iter_zip"]));
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
  expect(ratchet(results.slice(0, 2), new Map([["b.rs", "x"]]))).toEqual({
    regressions: [],
    fixed: [],
    worse: [],
    unanswered: [],
    answered: [],
    excluded: [],
    included: [],
  });
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
    ["d.rs", "node: different stdout"],
  ]);
  const now: Result[] = [
    { test: "a.rs", status: "fail", reason: "error: rust-js does not support unions yet" },
    { test: "b.rs", status: "fail", reason: "thread 'rustc' panicked at x.rs:1:2:" },
    { test: "c.rs", status: "fail", reason: "node: different stdout" },
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
  expect([...surprises(now, listed, new Map(), new Map([["f.rs", "has revisions"]]))].sort()).toEqual(["b.rs", "c.rs", "d.rs"]);
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
  const authority = { commit: "t1", tests: inventory };
  expect(validate([shard(1), shard(2)], known, "s1", authority)).toEqual([]);
  expect(validate([], known, "s1", authority)).toEqual(["there are no shards"]);
  expect(validate([[] as unknown as Shard], known, "s1", authority)).toEqual(["1 of the 1 files aren't a shard's record"]);
  // What's read from a file is checked, field by field: a status that isn't
  // one, or a failure with no reason, isn't counted. Found in review: a
  // status of `typo` merged as a run with no failures.
  const typo = shard(2, { results: [{ test: "b.rs", status: "typo" }, { test: "d.rs", status: "fail" }] as unknown as Result[] });
  expect(validate([shard(1), typo], known, "s1", authority)).toEqual(["results that aren't one: 2, as b.rs, d.rs"]);
  expect(validate([shard(1), shard(2, { of: "2" as unknown as number })], known, "s1", authority)).toEqual(["1 of the 2 files aren't a shard's record"]);
  expect(validate([shard(1), shard(2, { expected: [7] as unknown as string[] })], known, "s1", authority)).toEqual(["1 of the 2 files aren't a shard's record"]);
  expect(validate([shard(1)], known, "s1", authority)).toEqual(["shards missing, of 2: 2", "tests with no result: 2, as b.rs, d.rs"]);
  expect(validate([shard(1), shard(1), shard(2)], known, "s1", authority)).toEqual(["shards there more than once, of 2: 1", "tests with more than one result: 2, as a.rs, c.rs"]);
  expect(validate([shard(1), shard(2, { compiler: "c2" })], known, "s1", authority)).toEqual(["the shards ran with different compilers: c1, c2"]);
  expect(validate([shard(1), shard(2)], known, "s2", authority)).toEqual(["the shards ran source s1, and this is s2"]);
  expect(validate([shard(1), shard(2, { results: [] })], known, "s1", authority)).toEqual(["tests with no result: 2, as b.rs, d.rs"]);
  expect(validate([shard(1), shard(2, { expected: ["b.rs"], results: [{ test: "b.rs", status: "pass" }] })], known, "s1", authority)).toEqual([
    "shard 2 was to run other tests than its share",
    "tests with no result: 1, as d.rs",
  ]);
  expect(validate([shard(1), shard(2)], new Map([["gone.rs", "x"]]), "s1", authority)).toEqual(["known failures that aren't tests: 1, as gone.rs"]);
  expect(validate([shard(1, { inventory: [], expected: [], results: [] }), shard(2, { inventory: [], expected: [], results: [] })], new Map(), "s1", { commit: "t1", tests: [] })).toEqual(["there were no tests to run"]);
  // What they had to run is the checked-in inventory, not what they say:
  // a passing test left out of both a run's inventory and its results
  // isn't a whole run. Found in review: only the listed tests, and none that
  // passed, merged as a run.
  const without = inventory.filter((test) => test !== "c.rs");
  const short = (i: number): Shard => {
    const expected = without.filter((_, k) => k % 2 === i - 1);
    return { ...shard(i), inventory: without, expected, results: expected.map((test): Result => ({ test, status: "pass" })) };
  };
  expect(validate([short(1), short(2)], known, "s1", authority)).toEqual(["tests the inventory has that the run didn't: 1, as c.rs"]);
  expect(validate([shard(1), shard(2)], known, "s1", { commit: "t2", tests: inventory })).toEqual(["the shards ran rustc t1, and the inventory is of t2"]);
  expect(validate([shard(1), shard(2)], known, "s1", undefined)).toEqual(["there's no test/rustc-inventory.txt to check the run against: bless one"]);
  // A bless writes the inventory anew, for its diff to be reviewed.
  expect(validate([short(1), short(2)], known, "s1", authority, { bless: true })).toEqual([]);
  // A known failure that's no test is a run that isn't whole, a bless too,
  // unless it's of another rustc, whose tests rustc renamed or took out: the
  // new ones are the run's, and the bless's diff shows what went (ADR 0109).
  const gone = new Map([...known, ["gone.rs", "x"]]);
  expect(validate([shard(1), shard(2)], gone, "s1", authority, { bless: true })).toEqual(["known failures that aren't tests: 1, as gone.rs"]);
  expect(validate([shard(1), shard(2)], gone, "s1", { commit: "t0", tests: inventory }, { bless: true })).toEqual([]);
});

// What native Rust gives no answer for is listed too: a test that passed and
// no longer builds natively, or a listed one that now runs, isn't as listed.
// Found in review: a listed test that became a skip passed unseen.
test("a test native Rust newly gives no answer for, or newly does, isn't as listed", () => {
  const known = new Map([["a.rs", "error: rust-js does not support statics yet"]]);
  const native = new Map([["c.rs", "rustc: error: linking with `cc` failed"]]);
  const now: Result[] = [
    { test: "a.rs", status: "native", reason: "doesn't pass natively with overflow checks off: didn't finish in 10s" },
    { test: "b.rs", status: "native", reason: "rustc: error: linking with `cc` failed" },
    { test: "c.rs", status: "pass" },
    { test: "d.rs", status: "native", reason: "prints what changes from run to run" },
    { test: "e.rs", status: "skip", reason: "has revisions" },
  ];
  const { unanswered, answered } = ratchet(now, known, new Map([...native, ["d.rs", "prints what changes from run to run"]]));
  expect(unanswered.map((r) => r.test)).toEqual(["a.rs", "b.rs"]);
  expect(answered.map((r) => r.test)).toEqual(["c.rs"]);
  expect([...surprises(now, known, native, new Map([["e.rs", "has revisions"]]))].sort()).toEqual(["a.rs", "b.rs", "c.rs", "d.rs"]);
});

// What's out of scope is listed: a test that was run and now isn't, as a
// broader scope rule makes it, or one that's run now, isn't as listed.
// Found in review: a listed failure that became a skip passed unseen.
test("a test newly out of scope, or newly in it, isn't as listed", () => {
  const known = new Map([["a.rs", "error: rust-js does not support statics yet"]]);
  const outOfScope = new Map([["c.rs", "has revisions"], ["d.rs", "has revisions"]]);
  const now: Result[] = [
    { test: "a.rs", status: "skip", reason: "needs another crate" },
    { test: "b.rs", status: "skip", reason: "has revisions" },
    { test: "c.rs", status: "pass" },
    { test: "d.rs", status: "skip", reason: "has revisions" },
  ];
  const { excluded, included } = ratchet(now, known, new Map(), outOfScope);
  expect(excluded.map((r) => r.test)).toEqual(["a.rs", "b.rs"]);
  expect(included.map((r) => r.test)).toEqual(["c.rs"]);
  expect([...surprises(now, known, new Map(), outOfScope)].sort()).toEqual(["a.rs", "b.rs", "c.rs"]);
});

test("a bless writes the lists, but not a new crash or wrong answer without failing", () => {
  const results: Result[] = [
    { test: "a.rs", status: "fail", reason: "node: ended {\"error\":\"TypeError: x\"}" },
    { test: "b.rs", status: "fail", reason: "rustc panicked" },
    { test: "c.rs", status: "fail", reason: "error: rust-js does not support statics yet" },
    { test: "d.rs", status: "fail", reason: "node: different stdout" },
    { test: "e.rs", status: "pass" },
  ];
  // `a` was a clear rejection, `b` wasn't listed: both new, and not blessed quietly.
  // `c` is a rejection, and `d` was wrong already.
  const known = new Map([["a.rs", "error: rust-js does not support user implementations of this standard or external trait yet"], ["d.rs", "node: different stdout"]]);
  expect(unblessable(results, known).map((r) => r.test)).toEqual(["a.rs", "b.rs"]);
  expect(unblessable(results.slice(2), known)).toEqual([]);
});
