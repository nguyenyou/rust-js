// How a child process ended, for every program the tests build or run
// (ADR 0088): its exit code, the signal that stopped it, whether it ran out
// of time or printed more than it may, and what it printed. A run is judged
// on all of it, never on what it printed alone.

export type Exit = {
  code: number | null;
  signal: string | null;
  timedOut: boolean;
  overflowed: boolean;
  stdout: string;
  stderr: string;
};

// What a process may print before it's stopped: far more than any test's.
const maxBuffer = 16 * 1024 * 1024;
// How long output is still read after a process is stopped, if something it
// started, outside its group, holds it open.
const grace = 1000;

/** Stop `pid` and what it started: each process runs as the leader of a
 * group of its own, and the group is what's killed. */
function stopGroup(pid: number) {
  try {
    process.kill(-pid, "SIGKILL");
  } catch {
    // The group has ended already.
  }
}

/** `cmd`, stopped after `timeout` ms or `maxBuffer` bytes of output, with
 * this process's environment and `env`, which a change to `process.env`
 * wouldn't give it. */
export function runSync(cmd: string[], cwd: string, timeout: number, env: Record<string, string> = {}): Exit {
  const p = Bun.spawnSync(cmd, { cwd, stdout: "pipe", stderr: "pipe", timeout, maxBuffer, detached: true, env: { ...process.env, ...env } });
  // What it started and left running goes with it.
  stopGroup(p.pid);
  return {
    code: p.exitCode,
    signal: p.signalCode ?? null,
    timedOut: p.exitedDueToTimeout ?? false,
    overflowed: p.exitedDueToMaxBuffer ?? false,
    stdout: p.stdout.toString(),
    stderr: p.stderr.toString(),
  };
}

/** `cmd`, as `runSync` runs it, without blocking the others running. */
export async function run(cmd: string[], cwd: string, timeout: number, env: Record<string, string> = {}): Promise<Exit> {
  const p = Bun.spawn(cmd, { cwd, stdout: "pipe", stderr: "pipe", detached: true, env: { ...process.env, ...env } });
  let timedOut = false;
  let overflowed = false;
  const readers = [p.stdout.getReader(), p.stderr.getReader()];
  let cut: Timer | undefined;
  const stop = () => {
    stopGroup(p.pid);
    cut ??= setTimeout(() => readers.forEach((reader) => reader.cancel()), grace);
  };
  const timer = setTimeout(() => {
    timedOut = true;
    stop();
  }, timeout);
  const read = async (reader: ReadableStreamDefaultReader<Uint8Array>) => {
    const chunks: Uint8Array[] = [];
    let size = 0;
    for (let next = await reader.read(); !next.done; next = await reader.read()) {
      if (size + next.value.length > maxBuffer) {
        overflowed = true;
        stop();
        break;
      }
      chunks.push(next.value);
      size += next.value.length;
    }
    return Buffer.concat(chunks).toString();
  };
  const [stdout, stderr] = await Promise.all(readers.map(read));
  await p.exited;
  stopGroup(p.pid);
  clearTimeout(timer);
  clearTimeout(cut);
  return { code: p.exitCode, signal: p.signalCode ?? null, timedOut, overflowed, stdout, stderr };
}

/** Why a process didn't end as a program does, or nothing if it did: it
 * ran out of time, printed too much, or was stopped by a signal. */
export function stopped(exit: Exit, timeout: number): string | undefined {
  if (exit.timedOut) return `didn't finish in ${timeout / 1000}s`;
  if (exit.overflowed) return `printed more than ${maxBuffer / 1024 / 1024} MB`;
  if (exit.signal) return `killed by ${exit.signal}`;
  return undefined;
}

const errors = (stderr: string) =>
  stderr.split("\n").filter((line) => /^error(\[E\d+\])?:/.test(line) && !line.startsWith("error: aborting due to"));
const crashLine = (stderr: string) =>
  stderr.split("\n").find((line) => /^thread '[^']*'( \(\d+\))? panicked at|internal compiler error|unexpectedly panicked/.test(line));

/** How a compile that didn't succeed failed. rust-js rejects a program by
 * exiting 1 with errors of its own, each `error: rust-js ..`; anything
 * else crashed, whatever it said first: a panic, an internal compiler
 * error, a signal, running out of time, another exit code, or an error of
 * rustc's that native Rust didn't give. */
export function compileFailure(exit: Exit, timeout: number): { kind: "rejected" | "crashed"; reason: string } {
  const why = stopped(exit, timeout);
  if (why) return { kind: "crashed", reason: why };
  const crash = crashLine(exit.stderr);
  if (crash) return { kind: "crashed", reason: crash };
  const all = errors(exit.stderr);
  const first = all[0] ?? exit.stderr.trim().split("\n")[0] ?? "no output";
  if (exit.code !== 1) return { kind: "crashed", reason: `exited ${exit.code}: ${first}` };
  const other = all.find((line) => !line.startsWith("error: rust-js"));
  if (all.length === 0 || other) return { kind: "crashed", reason: other ?? first };
  return { kind: "rejected", reason: first };
}
