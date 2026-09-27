// Generated programs (ADR 0092): Rust that no one wrote, each from a seed,
// valid and deterministic by construction, to run as the corpus's are. And a
// reducer, which makes one that fails as small as it still fails.
//
//   seed ──► program ──► native and JS differ? ──► reduce ──► a few lines, and the seed

// A random number generator whose numbers a seed says: mulberry32.
export function random(seed: number) {
  let state = seed >>> 0;
  const next = () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const int = (n: number) => Math.floor(next() * n);
  const pick = <T>(items: readonly T[]): T => items[int(items.length)];
  return { next, int, pick, chance: (p: number) => next() < p };
}
type Random = ReturnType<typeof random>;

// `usize` is 32 bits in rust-js and 64 natively (ADR 0090): not generated.
export const intTypes = ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"] as const;
export type IntTy = (typeof intTypes)[number];
export type Ty = IntTy | "bool";

const bits = (ty: IntTy) => Number(ty.slice(1));
const signed = (ty: IntTy) => ty.startsWith("i");
const range = (ty: IntTy): [bigint, bigint] => {
  const n = BigInt(bits(ty));
  return signed(ty) ? [-(1n << (n - 1n)), (1n << (n - 1n)) - 1n] : [0n, (1n << n) - 1n];
};

export type Expr =
  | { kind: "lit"; ty: Ty; value: bigint | boolean }
  | { kind: "var"; ty: Ty; name: string }
  | { kind: "bin"; ty: IntTy; op: string; a: Expr; b: Expr }
  | { kind: "shift"; ty: IntTy; op: "<<" | ">>"; a: Expr; b: Expr }
  | { kind: "method"; ty: IntTy; name: string; a: Expr; b?: Expr }
  | { kind: "cast"; ty: IntTy; a: Expr }
  | { kind: "cmp"; ty: "bool"; op: string; a: Expr; b: Expr }
  | { kind: "logic"; ty: "bool"; op: "&&" | "||"; a: Expr; b: Expr }
  | { kind: "not"; ty: "bool"; a: Expr }
  | { kind: "if"; ty: Ty; c: Expr; a: Expr; b: Expr };

export type Stmt =
  | { kind: "let"; name: string; ty: Ty; value: Expr }
  | { kind: "assign"; name: string; ty: Ty; op: string; value: Expr }
  | { kind: "print"; format: string; value: Expr }
  | { kind: "if"; c: Expr; then: Stmt[]; else: Stmt[] }
  | { kind: "for"; name: string; n: number; body: Stmt[] };

export type Program = Stmt[];

// A loop's variable can be read, not written.
type Scope = { name: string; ty: Ty; mutable: boolean }[];

function literal(r: Random, ty: Ty): Expr {
  if (ty === "bool") return { kind: "lit", ty, value: r.chance(0.5) };
  const [lo, hi] = range(ty);
  // Where JS and Rust are most likely to part: the edges.
  const edges = [0n, 1n, 2n, lo, hi, lo + 1n, hi - 1n, hi / 2n, 1n << 31n, (1n << 53n) + 1n, (1n << 32n) - 1n];
  const values = signed(ty) ? [...edges, -1n, -2n, -(1n << 31n)] : edges;
  const inRange = values.filter((v) => v >= lo && v <= hi);
  const value = r.chance(0.7) ? r.pick(inRange) : lo + BigInt(r.int(Number(hi - lo > 1000n ? 1000n : hi - lo + 1n)));
  return { kind: "lit", ty, value };
}

function expr(r: Random, ty: Ty, scope: Scope, depth: number): Expr {
  const vars = scope.filter((v) => v.ty === ty);
  if (depth <= 0 || r.chance(0.25)) {
    return vars.length > 0 && r.chance(0.6) ? { kind: "var", ty, name: r.pick(vars).name } : literal(r, ty);
  }
  const sub = (t: Ty) => expr(r, t, scope, depth - 1);
  if (ty === "bool") {
    const choice = r.int(4);
    if (choice === 0) {
      const t = r.pick(intTypes);
      return { kind: "cmp", ty, op: r.pick(["==", "!=", "<", "<=", ">", ">="]), a: sub(t), b: sub(t) };
    }
    if (choice === 1) return { kind: "logic", ty, op: r.pick(["&&", "||"] as const), a: sub("bool"), b: sub("bool") };
    if (choice === 2) return { kind: "not", ty, a: sub("bool") };
    return { kind: "if", ty, c: sub("bool"), a: sub("bool"), b: sub("bool") };
  }
  switch (r.int(6)) {
    case 0:
    case 1:
      return { kind: "bin", ty, op: r.pick(["+", "-", "*", "/", "%", "&", "|", "^"]), a: sub(ty), b: sub(ty) };
    case 2:
      return { kind: "shift", ty, op: r.pick(["<<", ">>"] as const), a: sub(ty), b: sub(r.pick(intTypes)) };
    case 3: {
      const binary = ["wrapping_add", "wrapping_sub", "wrapping_mul", "saturating_add", "saturating_sub", "saturating_mul", "max", "min"];
      const unary = ["count_ones", "leading_zeros", "trailing_zeros", ...(signed(ty) ? ["abs"] : [])];
      if (r.chance(0.7)) return { kind: "method", ty, name: r.pick(binary), a: sub(ty), b: sub(ty) };
      const name = r.pick(unary);
      // What a count counts is a `u32`.
      if (name !== "abs") return { kind: "cast", ty, a: { kind: "method", ty: "u32", name, a: sub(ty) } };
      return { kind: "method", ty, name, a: sub(ty) };
    }
    case 4:
      return { kind: "cast", ty, a: sub(r.pick(intTypes)) };
    default:
      return { kind: "if", ty, c: sub("bool"), a: sub(ty), b: sub(ty) };
  }
}

function block(r: Random, scope: Scope, depth: number, counter: { n: number }, size: number): Stmt[] {
  const stmts: Stmt[] = [];
  const inner = [...scope];
  for (let i = 0; i < size; i++) {
    const choice = r.int(10);
    const writable = inner.filter((v) => v.mutable);
    if (choice < 3 || writable.length === 0) {
      const ty: Ty = r.chance(0.15) ? "bool" : r.pick(intTypes);
      const name = `v${counter.n++}`;
      stmts.push({ kind: "let", name, ty, value: expr(r, ty, inner, 3) });
      inner.push({ name, ty, mutable: true });
    } else if (choice < 5) {
      const target = r.pick(writable);
      const ops = target.ty === "bool" ? ["="] : ["=", "+=", "-=", "*=", "^=", "|=", "&="];
      stmts.push({ kind: "assign", name: target.name, ty: target.ty, op: r.pick(ops), value: expr(r, target.ty, inner, 2) });
    } else if (choice < 8) {
      const ty: Ty = r.chance(0.15) ? "bool" : r.pick(intTypes);
      const format = ty === "bool" ? r.pick(["{}", "{:?}"]) : r.pick(["{}", "{:?}", "{:x}", "{:#x}", "{:5}", "{:<4}|"]);
      stmts.push({ kind: "print", format, value: expr(r, ty, inner, 3) });
    } else if (choice === 8 && depth > 0) {
      stmts.push({ kind: "if", c: expr(r, "bool", inner, 2), then: block(r, inner, depth - 1, counter, 1 + r.int(3)), else: block(r, inner, depth - 1, counter, r.int(3)) });
    } else if (depth > 0) {
      const name = `k${counter.n++}`;
      stmts.push({ kind: "for", name, n: 1 + r.int(4), body: block(r, [...inner, { name, ty: "u32", mutable: false }], depth - 1, counter, 1 + r.int(3)) });
    }
  }
  return stmts;
}

/** The program seed `seed` says, the same each time. */
export function generate(seed: number): Program {
  const r = random(seed);
  return block(r, [], 2, { n: 0 }, 6 + r.int(8));
}

const suffix = (ty: Ty) => (ty === "bool" ? "" : ty);

function show(e: Expr): string {
  switch (e.kind) {
    case "lit":
      // Through `id`, so rustc can't work out an overflow and reject it.
      return typeof e.value === "boolean" ? `id(${e.value})` : `id(${e.value}${suffix(e.ty)})`;
    case "var":
      return e.name;
    case "bin":
    case "cmp":
    case "logic":
      return `(${show(e.a)} ${e.op} ${show(e.b)})`;
    case "shift":
      return `(${show(e.a)} ${e.op} ${show(e.b)})`;
    case "method":
      return e.b ? `${show(e.a)}.${e.name}(${show(e.b)})` : `${show(e.a)}.${e.name}()`;
    case "cast":
      return `(${show(e.a)} as ${e.ty})`;
    case "not":
      return `!${show(e.a)}`;
    case "if":
      return `(if ${show(e.c)} { ${show(e.a)} } else { ${show(e.b)} })`;
  }
}

function lines(stmts: Stmt[], indent: string): string[] {
  return stmts.flatMap((s): string[] => {
    switch (s.kind) {
      case "let":
        return [`${indent}let mut ${s.name}: ${s.ty} = ${show(s.value)};`];
      case "assign":
        return [`${indent}${s.name} ${s.op} ${show(s.value)};`];
      case "print":
        return [`${indent}println!("${s.format}", ${show(s.value)});`];
      case "if":
        return [
          `${indent}if ${show(s.c)} {`,
          ...lines(s.then, indent + "    "),
          `${indent}} else {`,
          ...lines(s.else, indent + "    "),
          `${indent}}`,
        ];
      case "for":
        return [`${indent}for ${s.name} in 0..id(${s.n}u32) {`, ...lines(s.body, indent + "    "), `${indent}}`];
    }
  });
}

/** The program as Rust: its statements in `main`, and `id`, which hides a
 * literal's value from rustc's checks. */
export function print(program: Program, seed?: number): string {
  return [
    ...(seed === undefined ? [] : [`// Generated from seed ${seed}.`]),
    "fn id<T>(x: T) -> T {",
    "    x",
    "}",
    "",
    "#[allow(unused, arithmetic_overflow, unconditional_panic)]",
    "fn main() {",
    ...lines(program, "    "),
    "}",
    "",
  ].join("\n");
}

// Smaller programs, each one change from `program`: a statement taken away,
// an `if` or a loop made what's inside it, an expression made one of its
// parts or a literal. Some are no longer valid Rust, which the caller finds.
function* smaller(program: Program): Generator<Program> {
  function* inBlock(stmts: Stmt[], rebuild: (next: Stmt[]) => Program): Generator<Program> {
    for (let i = 0; i < stmts.length; i++) {
      const s = stmts[i];
      const put = (replacement: Stmt[]) => rebuild([...stmts.slice(0, i), ...replacement, ...stmts.slice(i + 1)]);
      yield put([]);
      if (s.kind === "if") {
        yield put(s.then);
        yield put(s.else);
        yield* inBlock(s.then, (next) => put([{ ...s, then: next }]));
        yield* inBlock(s.else, (next) => put([{ ...s, else: next }]));
      } else if (s.kind === "for") {
        yield put(s.body);
        yield* inBlock(s.body, (next) => put([{ ...s, body: next }]));
      } else if (s.kind === "let" || s.kind === "assign" || s.kind === "print") {
        for (const value of simpler(s.value)) yield put([{ ...s, value }]);
      }
    }
  }
  yield* inBlock(program, (next) => next);
}

// Simpler expressions of the same type: its parts that have it, and a literal.
function* simpler(e: Expr): Generator<Expr> {
  if (e.kind === "lit" || e.kind === "var") return;
  for (const part of ["a", "b", "c"] as const) {
    const p = (e as Record<string, unknown>)[part] as Expr | undefined;
    if (p && p.ty === e.ty) yield p;
  }
  yield e.ty === "bool" ? { kind: "lit", ty: e.ty, value: false } : { kind: "lit", ty: e.ty, value: 0n };
  for (const part of ["a", "b", "c"] as const) {
    const p = (e as Record<string, unknown>)[part] as Expr | undefined;
    if (p) for (const q of simpler(p)) yield { ...e, [part]: q } as Expr;
  }
}

/** `program` made as small as it still `fails`: each change that keeps it
 * failing is kept, until none does. */
export async function reduce(program: Program, fails: (p: Program) => Promise<boolean>): Promise<Program> {
  let current = program;
  let changed = true;
  while (changed) {
    changed = false;
    for (const candidate of smaller(current)) {
      if (await fails(candidate)) {
        current = candidate;
        changed = true;
        break;
      }
    }
  }
  return current;
}

/** How many statements a program has, at every depth. */
export function size(program: Program): number {
  return program.reduce((n, s) => n + 1 + (s.kind === "if" ? size(s.then) + size(s.else) : s.kind === "for" ? size(s.body) : 0), 0);
}
