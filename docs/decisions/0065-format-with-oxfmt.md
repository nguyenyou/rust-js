# 0065. The JS is formatted as oxfmt formats it, and the source map follows

Status: Accepted. Extends [0018](0018-print-with-oxc.md).

## Context

oxc's printer (ADR 0018) writes each node the same way wherever it is: an
array of three items on three lines, an object of two fields on two, and a
long expression on one line of any length. Code a person writes, or one
that has been through Prettier, oxfmt or Biome, is laid out by width: what
fits on a line stays there, and what doesn't breaks where a reader expects.

Laying code out by width is what a formatter does, and Prettier's rules for
it are many (binary chains, a call's last argument, member chains, JSX).
oxfmt, oxc's formatter, gives Prettier's output. It formats text, not an
AST, and it writes no source map.

## Decision

**Print as before, then format the text with oxfmt's own formatter
(`oxc_formatter`), and move the source map to the formatted text.**

```text
  js::Module ──to_oxc──► printed JS + map ──oxc_formatter──► formatted JS
                             │                                    │
                           parse                                parse
                             ▼                                    ▼
                         nodes in order  ◄──── paired ────►  nodes in order
```

- **The options are oxfmt's defaults** (Prettier's, 100 columns), but for
  one: objects go on one line when they fit, Prettier's
  `objectWrap: "collapse"`. oxfmt keeps an object on several lines when the
  input has it so, to keep a layout a person chose; here the printer chose
  it.
- **The map is moved by pairing nodes.** Formatting changes where things
  are, not what they are, so both texts parse to the same nodes in the same
  order. A mapping at a node's start in the printed text moves to that
  node's start in the formatted one. Where the formatter adds nodes (JSX's
  `{" "}`), the pairing steps over them.
- **If the formatter fails, the printed text is kept**, with its map.
- `oxc_formatter` isn't published to crates.io, so every oxc crate comes
  from the git tag of the release rust-js pins (`crates_v0.151.0`), which
  keeps one copy of each. rust-js.wasm uses the same.

## Why

- **The output is oxfmt's, exactly**, so it reads as code a person
  formatted, and formatting it again changes nothing.
- **Debuggers still show the Rust**: the source map tests (eight JS → Rust
  probes) pass on the formatted output.
- **It's a small change.** The printer, and the layout it gives JSX, stay
  as they are; one file, `src/format.rs`, formats and moves the map.

## Alternatives

- **Our own printer, laying out by width.** The map would come straight
  from our spans, with no second parse. But it would mean writing Prettier's
  rules again, and the output would only ever be close to theirs.
- **oxfmt as a separate step** (its CLI, or in the Vite plugin). The map
  would be lost, and each place that writes JS would need it.

## Consequences

- Each module is parsed twice more, once before formatting and once after,
  to pair the nodes.
- rust-js.wasm is about 2.6 MB larger.
- Upgrading oxc means moving the git tag, in `Cargo.toml` and
  `wasm/Cargo.toml` both.
- A mapping inside a node, rather than at its start, keeps its offset from
  the node's start, which is right unless the formatter changed the text
  between them.
