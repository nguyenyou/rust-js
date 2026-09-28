// How a child process ended, and how a failed compile is told apart:
// rust-js's rejection, or a crash, whatever it said first.

import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { compileFailure, run, runSync, type Exit } from "./child";
import { fixture } from "./support";

const exit = (code: number | null, stderr: string, more: Partial<Exit> = {}): Exit => ({
  code,
  signal: null,
  timedOut: false,
  overflowed: false,
  stdout: "",
  stderr,
  ...more,
});
const rejection = "error: rust-js does not support 128-bit integers yet\n --> case.rs:1:1\n\nerror: aborting due to 1 previous error\n";

test("rust-js's own errors, and exit 1, are a rejection", () => {
  expect(compileFailure(exit(1, rejection), 1000)).toEqual({ kind: "rejected", reason: "error: rust-js does not support 128-bit integers yet" });
  expect(compileFailure(exit(1, "error: rust-js: generated trait implementation name `x` collides\n"), 1000).kind).toBe("rejected");
});

test("a rejection followed by a crash is a crash", () => {
  const panicked = `${rejection}thread 'rustc' panicked at src/lower.rs:1:1:\nboom\n`;
  expect(compileFailure(exit(101, panicked), 1000)).toEqual({ kind: "crashed", reason: "thread 'rustc' panicked at src/lower.rs:1:1:" });
  expect(compileFailure(exit(1, panicked), 1000).kind).toBe("crashed");
  // As rustc says it, with the thread's number.
  const numbered = `${rejection}thread 'rustc' (21196015) panicked at src/lower/representation.rs:226:34:\n`;
  expect(compileFailure(exit(101, numbered), 1000).reason).toBe("thread 'rustc' (21196015) panicked at src/lower/representation.rs:226:34:");
  expect(compileFailure(exit(101, "error: internal compiler error: oops\n"), 1000).reason).toBe("error: internal compiler error: oops");
});

test("another exit code, rustc's own error, a signal or a deadline is a crash", () => {
  expect(compileFailure(exit(2, rejection), 1000)).toEqual({ kind: "crashed", reason: "exited 2: error: rust-js does not support 128-bit integers yet" });
  expect(compileFailure(exit(1, `${rejection}error[E0277]: the trait bound is not satisfied\n`), 1000)).toEqual({
    kind: "crashed",
    reason: "error[E0277]: the trait bound is not satisfied",
  });
  expect(compileFailure(exit(1, ""), 1000).kind).toBe("crashed");
  expect(compileFailure(exit(null, rejection, { signal: "SIGSEGV" }), 1000)).toEqual({ kind: "crashed", reason: "killed by SIGSEGV" });
  expect(compileFailure(exit(null, rejection, { signal: "SIGTERM", timedOut: true }), 2000)).toEqual({ kind: "crashed", reason: "didn't finish in 2s" });
});

test("a process that doesn't end, or prints without end, is stopped and says so", async () => {
  const slow = await run(["sh", "-c", "sleep 30"], ".", 200);
  expect([slow.timedOut, slow.signal]).toEqual([true, "SIGKILL"]);
  expect(runSync(["sh", "-c", "sleep 30"], ".", 200).timedOut).toBe(true);
  const loud = await run(["sh", "-c", "yes"], ".", 10_000);
  expect([loud.overflowed, loud.timedOut]).toEqual([true, false]);
  expect(runSync(["sh", "-c", "yes"], ".", 10_000).overflowed).toBe(true);
  const failed = await run(["sh", "-c", "echo out; echo err >&2; exit 3"], ".", 10_000);
  expect(failed).toEqual({ code: 3, signal: null, timedOut: false, overflowed: false, stdout: "out\n", stderr: "err\n" });
});

// What a process started, holding its output open, is stopped with it, so
// a deadline is kept. Found in review: `sleep 3 & wait` took 3s of 100ms.
test("a process's descendants are stopped with it, and its deadline kept", async () => {
  const pidFile = join(fixture("child-descendant"), "pid");
  const started = Date.now();
  const slow = await run(["sh", "-c", `sleep 30 & echo $! > ${pidFile}; wait`], ".", 100);
  expect(slow.timedOut).toBe(true);
  expect(Date.now() - started).toBeLessThan(2000);
  const alive = (pid: number) => {
    try {
      process.kill(pid, 0);
      return true;
    } catch {
      return false;
    }
  };
  await Bun.sleep(50);
  expect(alive(Number(readFileSync(pidFile, "utf8")))).toBe(false);
  const sync = runSync(["sh", "-c", `sleep 30 & echo $! > ${pidFile}; wait`], ".", 100);
  expect(sync.timedOut).toBe(true);
  await Bun.sleep(50);
  expect(alive(Number(readFileSync(pidFile, "utf8")))).toBe(false);
});

// A setting for a child is given to it, as `process.env` changed later
// isn't. Found by the mutation runner, whose tests never saw its compiler.
test("a process gets the environment it's given", async () => {
  expect(runSync(["sh", "-c", "echo $CHILD_SETTING"], ".", 10_000, { CHILD_SETTING: "given" }).stdout).toBe("given\n");
  expect((await run(["sh", "-c", "echo $CHILD_SETTING"], ".", 10_000, { CHILD_SETTING: "given" })).stdout).toBe("given\n");
});
