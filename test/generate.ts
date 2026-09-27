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
// An integer, a `bool`, `Vec<i32>`, `Option<u8>`, the struct `S`, or a
// closure from an integer to one, `fn(i32)`.
export type Ty = string;

const isInt = (ty: Ty): ty is IntTy => (intTypes as readonly string[]).includes(ty);
const inside = (ty: Ty, outer: "Vec" | "Option"): IntTy | undefined => {
  const m = new RegExp(`^${outer}<(\\w+)>$`).exec(ty);
  return m && isInt(m[1]) ? m[1] : undefined;
};
// What's `Copy` can be read as it is; a `Vec` is read through `clone()`, so
// it's never moved from the variable that holds it.
const isCopy = (ty: Ty) => inside(ty, "Vec") === undefined;

// The struct every program has, whose fields are of three widths.
export const struct = { name: "S", fields: [["a", "i32"], ["b", "u8"], ["c", "i64"]] as [string, IntTy][] };

const bits = (ty: IntTy) => Number(ty.slice(1));
const signed = (ty: IntTy) => ty.startsWith("i");
const range = (ty: IntTy): [bigint, bigint] => {
  const n = BigInt(bits(ty));
  return signed(ty) ? [-(1n << (n - 1n)), (1n << (n - 1n)) - 1n] : [0n, (1n << n) - 1n];
};

export type Expr =
  | { kind: "lit"; ty: Ty; value: bigint | boolean }
  | { kind: "zero"; ty: Ty }
  | { kind: "var"; ty: Ty; name: string }
  | { kind: "bin"; ty: Ty; op: string; a: Expr; b: Expr }
  | { kind: "shift"; ty: Ty; op: "<<" | ">>"; a: Expr; b: Expr }
  | { kind: "method"; ty: Ty; name: string; a: Expr; b?: Expr }
  | { kind: "cast"; ty: Ty; a: Expr }
  | { kind: "cmp"; ty: Ty; op: string; a: Expr; b: Expr }
  | { kind: "logic"; ty: Ty; op: "&&" | "||"; a: Expr; b: Expr }
  | { kind: "not"; ty: Ty; a: Expr }
  | { kind: "if"; ty: Ty; c: Expr; a: Expr; b: Expr }
  // `vec![a, b]`, `v[i]`, `S { a, b, c }`, `s.a`, `f(a)`, and what a
  // `Vec` or an `Option` can say, with or without a closure.
  | { kind: "vec"; ty: Ty; items: Expr[] }
  | { kind: "index"; ty: Ty; a: Expr; at: number }
  | { kind: "struct"; ty: Ty; a: Expr; b: Expr; c: Expr }
  | { kind: "field"; ty: Ty; a: Expr; field: string }
  | { kind: "call"; ty: Ty; name: string; a: Expr }
  | { kind: "some"; ty: Ty; a: Expr }
  | { kind: "use"; ty: Ty; form: string; a: Expr; b?: Expr };

export type Stmt =
  | { kind: "let"; name: string; ty: Ty; value: Expr }
  | { kind: "assign"; name: string; ty: Ty; op: string; value: Expr }
  | { kind: "print"; format: string; value: Expr }
  | { kind: "if"; c: Expr; then: Stmt[]; else: Stmt[] }
  | { kind: "for"; name: string; n: number; body: Stmt[] }
  | { kind: "for-each"; name: string; items: string; body: Stmt[] }
  | { kind: "if-let"; name: string; value: Expr; then: Stmt[]; else: Stmt[] }
  | { kind: "vec-op"; name: string; op: "push" | "pop" | "sort" | "reverse"; value?: Expr }
  | { kind: "set"; name: string; index?: number; field?: string; value: Expr }
  | { kind: "closure"; name: string; param: IntTy; body: Expr };

export type Program = Stmt[];

// A loop's or a pattern's variable can be read, not written.
type Scope = { name: string; ty: Ty; mutable: boolean }[];

function literal(r: Random, ty: Ty): Expr {
  if (ty === "bool") return { kind: "lit", ty, value: r.chance(0.5) };
  if (!isInt(ty)) return { kind: "zero", ty };
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
  const sub = (t: Ty) => expr(r, t, scope, depth - 1);
  const holding = (outer: "Vec" | "Option", elem: IntTy) => scope.filter((v) => v.ty === `${outer}<${elem}>`);
  if (depth <= 0 || r.chance(0.2)) {
    if (vars.length > 0 && r.chance(0.6)) return { kind: "var", ty, name: r.pick(vars).name };
    if (ty === "S") return { kind: "struct", ty, a: literal(r, "i32"), b: literal(r, "u8"), c: literal(r, "i64") };
    // Mostly something to work on: a few items, or `Some` of one.
    const vecOf = inside(ty, "Vec"), optionOf = inside(ty, "Option");
    if (vecOf) return { kind: "vec", ty, items: Array.from({ length: r.chance(0.1) ? 0 : 1 + r.int(4) }, () => literal(r, vecOf)) };
    if (optionOf && r.chance(0.7)) return { kind: "some", ty, a: literal(r, optionOf) };
    return literal(r, ty);
  }
  if (ty === "bool") {
    const choice = r.int(6);
    if (choice === 0) {
      const t = r.pick(intTypes);
      return { kind: "cmp", ty, op: r.pick(["==", "!=", "<", "<=", ">", ">="]), a: sub(t), b: sub(t) };
    }
    if (choice === 1) return { kind: "logic", ty, op: r.pick(["&&", "||"] as const), a: sub("bool"), b: sub("bool") };
    if (choice === 2) return { kind: "not", ty, a: sub("bool") };
    if (choice === 3) {
      const elem = r.pick(intTypes);
      return r.chance(0.5)
        ? { kind: "use", ty, form: "contains", a: sub(`Vec<${elem}>`), b: sub(elem) }
        : { kind: "use", ty, form: r.pick(["is_empty"]), a: sub(`Vec<${elem}>`) };
    }
    if (choice === 4) return { kind: "use", ty, form: r.pick(["is_some", "is_none"]), a: sub(`Option<${r.pick(intTypes)}>`) };
    if (choice === 5 && r.chance(0.5)) return { kind: "cmp", ty, op: r.pick(["==", "!="]), a: sub("S"), b: sub("S") };
    return { kind: "if", ty, c: sub("bool"), a: sub("bool"), b: sub("bool") };
  }
  const vecOf = inside(ty, "Vec");
  if (vecOf) {
    switch (r.int(4)) {
      case 0:
        return { kind: "vec", ty, items: Array.from({ length: r.int(5) }, () => sub(vecOf)) };
      case 1:
        return { kind: "use", ty, form: "map", a: sub(ty), b: sub(vecOf) };
      case 2:
        return { kind: "use", ty, form: "filter", a: sub(ty), b: sub(vecOf) };
      default:
        return { kind: "if", ty, c: sub("bool"), a: sub(ty), b: sub(ty) };
    }
  }
  const optionOf = inside(ty, "Option");
  if (optionOf) {
    switch (r.int(5)) {
      case 0:
        return { kind: "some", ty, a: sub(optionOf) };
      case 1:
        return { kind: "method", ty, name: r.pick(["checked_add", "checked_sub", "checked_mul", "checked_div"]), a: sub(optionOf), b: sub(optionOf) };
      case 2:
        return { kind: "use", ty, form: r.pick(["first", "last", "max", "min"]), a: sub(`Vec<${optionOf}>`) };
      case 3:
        return { kind: "use", ty, form: "map-option", a: sub(ty), b: sub(optionOf) };
      default:
        return { kind: "zero", ty };
    }
  }
  if (ty === "S") return { kind: "struct", ty, a: sub("i32"), b: sub("u8"), c: sub("i64") };
  // An integer.
  const int = ty as IntTy;
  const closures = scope.filter((v) => v.ty === `fn(${int})`);
  switch (r.int(9)) {
    case 0:
    case 1:
      return { kind: "bin", ty, op: r.pick(["+", "-", "*", "/", "%", "&", "|", "^"]), a: sub(ty), b: sub(ty) };
    case 2:
      return { kind: "shift", ty, op: r.pick(["<<", ">>"] as const), a: sub(ty), b: sub(r.pick(intTypes)) };
    case 3: {
      const binary = ["wrapping_add", "wrapping_sub", "wrapping_mul", "saturating_add", "saturating_sub", "saturating_mul", "max", "min"];
      const unary = ["count_ones", "leading_zeros", "trailing_zeros", ...(signed(int) ? ["abs"] : [])];
      if (r.chance(0.7)) return { kind: "method", ty, name: r.pick(binary), a: sub(ty), b: sub(ty) };
      const name = r.pick(unary);
      // What a count counts is a `u32`.
      if (name !== "abs") return { kind: "cast", ty, a: { kind: "method", ty: "u32", name, a: sub(ty) } };
      return { kind: "method", ty, name, a: sub(ty) };
    }
    case 4:
      return { kind: "cast", ty, a: sub(r.pick(intTypes)) };
    case 5: {
      // From a `Vec`: an item, which may not be there and panic, its sum, or
      // its length.
      const choice = r.int(3);
      if (choice === 0) return { kind: "index", ty, a: sub(`Vec<${int}>`), at: r.chance(0.8) ? 0 : r.int(4) };
      if (choice === 1) return { kind: "use", ty, form: "sum", a: sub(`Vec<${int}>`) };
      return { kind: "cast", ty, a: { kind: "use", ty: "usize", form: "len", a: sub(`Vec<${r.pick(intTypes)}>`) } };
    }
    case 6:
      return { kind: "use", ty, form: "unwrap_or", a: sub(`Option<${int}>`), b: sub(ty) };
    case 7: {
      const field = struct.fields.find(([, t]) => t === int);
      if (field) return { kind: "field", ty, a: sub("S"), field: field[0] };
      if (closures.length > 0) return { kind: "call", ty, name: r.pick(closures).name, a: sub(ty) };
      return { kind: "if", ty, c: sub("bool"), a: sub(ty), b: sub(ty) };
    }
    default:
      if (closures.length > 0 && r.chance(0.5)) return { kind: "call", ty, name: r.pick(closures).name, a: sub(ty) };
      return { kind: "if", ty, c: sub("bool"), a: sub(ty), b: sub(ty) };
  }
}

// A value's type for a new variable: mostly integers, and the rest.
function valueType(r: Random): Ty {
  const n = r.next();
  if (n < 0.55) return r.pick(intTypes);
  if (n < 0.65) return "bool";
  if (n < 0.8) return `Vec<${r.pick(intTypes)}>`;
  if (n < 0.92) return `Option<${r.pick(intTypes)}>`;
  return "S";
}

function block(r: Random, scope: Scope, depth: number, counter: { n: number }, size: number): Stmt[] {
  const stmts: Stmt[] = [];
  const inner = [...scope];
  for (let i = 0; i < size; i++) {
    const writable = inner.filter((v) => v.mutable);
    const vecs = writable.filter((v) => inside(v.ty, "Vec"));
    const choice = r.int(14);
    if (choice < 3 || writable.length === 0) {
      const ty = valueType(r);
      const name = `v${counter.n++}`;
      stmts.push({ kind: "let", name, ty, value: expr(r, ty, inner, 3) });
      inner.push({ name, ty, mutable: true });
    } else if (choice < 5) {
      const target = r.pick(writable);
      const ops = isInt(target.ty) ? ["=", "+=", "-=", "*=", "^=", "|=", "&="] : ["="];
      stmts.push({ kind: "assign", name: target.name, ty: target.ty, op: r.pick(ops), value: expr(r, target.ty, inner, 2) });
    } else if (choice < 8) {
      const ty = r.chance(0.7) ? r.pick(intTypes) : valueType(r);
      const format = isInt(ty) ? r.pick(["{}", "{:?}", "{:x}", "{:#x}", "{:5}", "{:<4}|"]) : ty === "bool" ? r.pick(["{}", "{:?}"]) : "{:?}";
      stmts.push({ kind: "print", format, value: expr(r, ty, inner, 3) });
    } else if (choice === 8 && vecs.length > 0) {
      const v = r.pick(vecs);
      const elem = inside(v.ty, "Vec")!;
      const op = r.pick(["push", "push", "pop", "sort", "reverse"] as const);
      stmts.push({ kind: "vec-op", name: v.name, op, value: op === "push" ? expr(r, elem, inner, 2) : undefined });
    } else if (choice === 9) {
      const structs = writable.filter((v) => v.ty === "S");
      if (vecs.length > 0 && r.chance(0.5)) {
        const v = r.pick(vecs);
        stmts.push({ kind: "set", name: v.name, index: r.chance(0.8) ? 0 : r.int(4), value: expr(r, inside(v.ty, "Vec")!, inner, 2) });
      } else if (structs.length > 0) {
        const [field, t] = r.pick(struct.fields);
        stmts.push({ kind: "set", name: r.pick(structs).name, field, value: expr(r, t, inner, 2) });
      }
    } else if (choice === 10) {
      // A closure captures what's `Copy`, by value, so a later write can't
      // conflict with it.
      const param = r.pick(intTypes);
      const name = `f${counter.n++}`;
      const x = `x${counter.n++}`;
      const captures = inner.filter((v) => isInt(v.ty) || v.ty === "bool").map((v) => ({ ...v, mutable: false }));
      const body = expr(r, param, [...captures, { name: x, ty: param, mutable: false }], 2);
      stmts.push({ kind: "closure", name, param, body: substitute(body, x) });
      inner.push({ name, ty: `fn(${param})`, mutable: false });
    } else if (choice === 11 && depth > 0) {
      stmts.push({ kind: "if", c: expr(r, "bool", inner, 2), then: block(r, inner, depth - 1, counter, 1 + r.int(3)), else: block(r, inner, depth - 1, counter, r.int(3)) });
    } else if (choice === 12 && depth > 0) {
      const elem = r.pick(intTypes);
      const name = `x${counter.n++}`;
      stmts.push({
        kind: "if-let",
        name,
        value: expr(r, `Option<${elem}>`, inner, 2),
        then: block(r, [...inner, { name, ty: elem, mutable: false }], depth - 1, counter, 1 + r.int(3)),
        else: block(r, inner, depth - 1, counter, r.int(2)),
      });
    } else if (depth > 0) {
      const name = `k${counter.n++}`;
      const lists = inner.filter((v) => inside(v.ty, "Vec"));
      if (lists.length > 0 && r.chance(0.4)) {
        const list = r.pick(lists);
        stmts.push({ kind: "for-each", name, items: list.name, body: block(r, [...inner, { name, ty: inside(list.ty, "Vec")!, mutable: false }], depth - 1, counter, 1 + r.int(3)) });
      } else {
        stmts.push({ kind: "for", name, n: 1 + r.int(4), body: block(r, [...inner, { name, ty: "u32", mutable: false }], depth - 1, counter, 1 + r.int(3)) });
      }
    }
  }
  return stmts;
}

// A closure's parameter is `x`, whatever its generated name was.
function substitute(e: Expr, name: string): Expr {
  if (e.kind === "var") return e.name === name ? { ...e, name: "x" } : e;
  const out: Record<string, unknown> = { ...e };
  for (const [key, value] of Object.entries(e)) {
    if (value && typeof value === "object" && "kind" in value) out[key] = substitute(value as Expr, name);
    if (Array.isArray(value)) out[key] = value.map((item) => substitute(item as Expr, name));
  }
  return out as Expr;
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
    case "zero":
      if (inside(e.ty, "Vec")) return `Vec::<${inside(e.ty, "Vec")}>::new()`;
      if (inside(e.ty, "Option")) return `None::<${inside(e.ty, "Option")}>`;
      if (e.ty === "S") return "(S { a: 0, b: 0, c: 0 })";
      return e.ty === "bool" ? "false" : `id(0${e.ty})`;
    case "var":
      return isCopy(e.ty) ? e.name : `${e.name}.clone()`;
    case "bin":
    case "cmp":
    case "logic":
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
    case "vec":
      return e.items.length === 0 ? `Vec::<${inside(e.ty, "Vec")}>::new()` : `vec![${e.items.map(show).join(", ")}]`;
    case "index":
      return `${show(e.a)}[id(${e.at}usize)]`;
    // In parentheses, as a struct literal can't begin an `if`'s condition.
    case "struct":
      return `(S { a: ${show(e.a)}, b: ${show(e.b)}, c: ${show(e.c)} })`;
    case "field":
      return `${show(e.a)}.${e.field}`;
    case "call":
      return `${e.name}(${show(e.a)})`;
    case "some":
      return `Some(${show(e.a)})`;
    case "use": {
      const a = show(e.a), b = e.b ? show(e.b) : "";
      switch (e.form) {
        case "contains":
          return `${a}.contains(&${b})`;
        case "is_empty":
        case "is_some":
        case "is_none":
          return `${a}.${e.form}()`;
        case "len":
          return `${a}.len()`;
        case "sum":
          return `${a}.iter().sum::<${e.ty}>()`;
        case "first":
        case "last":
          return `${a}.${e.form}().copied()`;
        case "max":
        case "min":
          return `${a}.iter().copied().${e.form}()`;
        // Their items are `e`, which no generated variable is, so a closure's
        // `x` isn't hidden by it.
        case "map":
          return `${a}.iter().map(|e| e.wrapping_add(${b})).collect::<${e.ty}>()`;
        case "filter":
          return `${a}.into_iter().filter(|e| *e > ${b}).collect::<${e.ty}>()`;
        case "map-option":
          return `${a}.map(|e| e.wrapping_mul(${b}))`;
        case "unwrap_or":
          return `${a}.unwrap_or(${b})`;
      }
      throw new Error(`no form ${e.form}`);
    }
  }
}

function lines(stmts: Stmt[], indent: string): string[] {
  const nested = (body: Stmt[]) => lines(body, indent + "    ");
  return stmts.flatMap((s): string[] => {
    switch (s.kind) {
      case "let":
        return [`${indent}let mut ${s.name}: ${s.ty} = ${show(s.value)};`];
      case "assign":
        return [`${indent}${s.name} ${s.op} ${show(s.value)};`];
      case "print":
        return [`${indent}println!("${s.format}", ${show(s.value)});`];
      case "if":
        return [`${indent}if ${show(s.c)} {`, ...nested(s.then), `${indent}} else {`, ...nested(s.else), `${indent}}`];
      case "for":
        return [`${indent}for ${s.name} in 0..id(${s.n}u32) {`, ...nested(s.body), `${indent}}`];
      case "for-each":
        return [`${indent}for ${s.name} in ${s.items}.clone() {`, ...nested(s.body), `${indent}}`];
      case "if-let":
        return [`${indent}if let Some(${s.name}) = ${show(s.value)} {`, ...nested(s.then), `${indent}} else {`, ...nested(s.else), `${indent}}`];
      case "vec-op":
        return [`${indent}${s.name}.${s.op}(${s.value ? show(s.value) : ""});`];
      case "set":
        return [`${indent}${s.name}${s.field !== undefined ? `.${s.field}` : `[id(${s.index}usize)]`} = ${show(s.value)};`];
      case "closure":
        return [`${indent}let ${s.name} = move |x: ${s.param}| -> ${s.param} { ${show(s.body)} };`];
    }
  });
}

/** The program as Rust: its statements in `main`, `id`, which hides a
 * literal's value from rustc's checks, and the struct `S`. */
export function print(program: Program, seed?: number): string {
  return [
    ...(seed === undefined ? [] : [`// Generated from seed ${seed}.`]),
    "fn id<T>(x: T) -> T {",
    "    x",
    "}",
    "",
    "#[derive(Debug, Clone, Copy, PartialEq)]",
    `struct S {`,
    ...struct.fields.map(([name, ty]) => `    ${name}: ${ty},`),
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
      if (s.kind === "if" || s.kind === "if-let") {
        yield put(s.then);
        yield put(s.else);
        yield* inBlock(s.then, (next) => put([{ ...s, then: next }]));
        yield* inBlock(s.else, (next) => put([{ ...s, else: next }]));
      } else if (s.kind === "for" || s.kind === "for-each") {
        yield put(s.body);
        yield* inBlock(s.body, (next) => put([{ ...s, body: next }]));
      }
      if ("value" in s && s.value) for (const value of simpler(s.value)) yield put([{ ...s, value } as Stmt]);
      if (s.kind === "closure") for (const body of simpler(s.body)) yield put([{ ...s, body }]);
    }
  }
  yield* inBlock(program, (next) => next);
}

// Simpler expressions of the same type: its parts that have it, and a
// literal; then each part made simpler.
function* simpler(e: Expr): Generator<Expr> {
  if (e.kind === "lit" || e.kind === "var" || e.kind === "zero") return;
  const parts = Object.entries(e).filter(([, v]) => v && typeof v === "object" && "kind" in v) as [string, Expr][];
  for (const [, p] of parts) if (p.ty === e.ty) yield p;
  yield e.ty === "bool" ? { kind: "lit", ty: e.ty, value: false } : isInt(e.ty) ? { kind: "lit", ty: e.ty, value: 0n } : { kind: "zero", ty: e.ty };
  if (e.kind === "vec") {
    for (let i = 0; i < e.items.length; i++) yield { ...e, items: [...e.items.slice(0, i), ...e.items.slice(i + 1)] };
  }
  for (const [key, p] of parts) for (const q of simpler(p)) yield { ...e, [key]: q } as Expr;
}

/** `program` made as small as it still `fails`: each change that keeps it
 * failing is kept, until none does. A change is kept only if it makes the
 * program shorter, so there's an end to them. */
export async function reduce(program: Program, fails: (p: Program) => Promise<boolean>): Promise<Program> {
  let current = program;
  let changed = true;
  while (changed) {
    changed = false;
    const length = print(current).length;
    for (const candidate of smaller(current)) {
      if (print(candidate).length < length && (await fails(candidate))) {
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
  return program.reduce((n, s) => {
    const inner = s.kind === "if" || s.kind === "if-let" ? size(s.then) + size(s.else) : s.kind === "for" || s.kind === "for-each" ? size(s.body) : 0;
    return n + 1 + inner;
  }, 0);
}
