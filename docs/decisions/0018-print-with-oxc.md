# 0018. Print with oxc, through one adapter file, and emit source maps

Status: Accepted. Supersedes the printer half of [0007](0007-js-ast-and-printer.md).

## Context

Our own printer (`js.rs`, ~320 lines) handled precedence and indentation
well, but three needs were coming:

- **Source maps**, so a debugger can step through the Rust, not the JS.
- **Correct string escaping**, which string literals will need. `{:?}` isn't
  JS escaping.
- Eventually **minified output**, and maybe `.d.ts` typings.

[oxc](https://oxc.rs) is a JavaScript toolchain written in Rust. Its code
generator, `oxc_codegen`, already handles precedence, escaping, number
formatting, minification and source maps.

## Decision

Keep our small JS AST, and convert it to oxc's AST in **one file**, which
oxc then prints:

```
THIR ──lower.rs──► js.rs AST ──to_oxc.rs──► oxc AST ──oxc_codegen──► .js + .js.map
                   (ours, with spans)       (the only file that uses oxc)
```

- **Every node in our AST carries a span**: byte offsets into the `.rs` file.
  `Span::NONE` (empty) means "no mapping", which oxc skips.
- **The source map points into Rust** through one trick: oxc computes each
  mapping's line and column from `program.source_text` plus the node's span.
  We pass the Rust file as `source_text`, and spans that index into it. oxc
  doesn't care that the "source" isn't JavaScript.
- **Pinned versions**: `oxc_* = "=0.151.0"`, `oxc_sourcemap = "=8.1.2"`.
- rust-js writes `out.js` and `out.js.map`. The JS ends with
  `//# sourceMappingURL=out.js.map`. The map names the Rust file relative to
  itself (`../examples/fib.rs`) and embeds its text (`sourcesContent`).

## Why

- **Our AST stays small and pleasant.** `lower.rs` keeps writing
  `Expr::bin(Op::Add, l, r)`. oxc's builder needs an arena, a lifetime, and
  a dozen arguments per function node. That verbosity lives only in `to_oxc.rs`.
- **oxc churn is contained.** oxc is pre-1.0. Between versions it renamed its
  whole builder API (`ast.expression_binary(..)` became
  `Expression::new_binary_expression(..)`) and split export nodes apart. When
  it changes again, we fix one ~300-line file.
- **Source maps nearly for free.** oxc already tracks output positions. We
  only had to supply spans.

## Details worth knowing

- **Rust spans → file offsets** (`FnCx::js_span`): rustc numbers bytes
  across all loaded files, so we subtract the root file's start. Code from
  a macro or desugaring maps to where it was written (`source_callsite`), so
  a `while` loop maps to the `while` keyword. Spans outside the root file get
  `NONE`.
- **rustc's copy of the source**, not a fresh read from disk: rustc
  normalizes the text it parses (it strips a BOM and turns CRLF into LF), and
  our byte offsets are into *that* text.
- **The prelude** (header comment and runtime helpers) is plain text above
  oxc's output. It has no mappings.
- **Blank lines between functions**: oxc prints top-level functions back to
  back. `to_oxc.rs` inserts a blank line before each function, then rebuilds
  the map with every mapping moved down by the number of lines inserted
  above it (`shift_lines`). The prelude shift uses the same mechanism.
- **Where a mapping points**: the outermost JS node made for a Rust
  expression gets that expression's span (`Expr::or_at`). Helper nodes like a
  `| 0` wrapper share it. Statements get their Rust statement's span.
  `return a;` from `break a;` maps to the `a`, on the right line.

## Alternatives

- **Keep our printer, add source maps ourselves**: we'd reimplement
  position tracking, VLQ encoding and string escaping, all of which oxc
  already does well.
- **Build oxc's AST directly in `lower.rs`**: one fewer layer, but it spreads
  the verbose API and oxc's version churn through the whole compiler.
- **swc**: a mature alternative with the same capabilities. We went with
  oxc. Because of the adapter, switching later would touch only
  `to_oxc.rs`.

## Consequences

- **Tests**: `bun test` now checks the map, not just the behavior: the
  sources entry, embedded content, no mappings in the prelude, every
  mapping inside the Rust file, and eight JS-snippet → Rust-snippet probes.
  A deliberate mutation (dropping the line shift) makes it fail, so it
  guards real behavior.
- **Build cost**: oxc adds dependencies. A clean build is slower. Incremental
  builds are unaffected.
- **Known gap**: renamed variables (`x` → `x$1`, see [0010](0010-naming-and-scopes.md))
  aren't reliably recorded under their Rust names. oxc skips a mapping at the
  same source position as the previous one, and an identifier often starts
  where its enclosing expression does.
- **Known gap**: declared names (`const x = ..`, parameters) have no span of
  their own. The whole statement is mapped instead, which is enough for
  line-level stepping.
- Minified output is now one option away (`CodegenOptions::minify`), but isn't
  exposed yet.
