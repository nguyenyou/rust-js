# 0072: JSX syntax shared by native and WASM

Status: Accepted

## Context

React already lowers to readable JSX, with Vite Fast Refresh and Rust source
maps. Builder chains become awkward for nested views. We want familiar markup
without a second language implementation for the browser compiler.

## Decision

Expand `jsx! { <tag>...</tag> }` in the compiler's
`after_crate_root_parsing` callback. Parse tags and attributes ourselves, retain
the tokens and spans of embedded Rust, and hand the expanded expressions back
to rustc. Keep the existing typed bindings and JSX lowering as the single
semantic implementation.

```
Rust + jsx! -> syntax expansion -> rustc checks -> existing JSX lowering
                                                       |
                                                       v
                                               .jsx + Rust source map
                                                       |
                                                       v
                                              Vite / React Fast Refresh
```

The callback sees only the root module. The expansion pass loads configured
out-of-line modules with rustc's parser and module path resolver, respecting
`cfg`, `cfg_attr`, inline module directories and `#[path]`. rustc still performs
macro expansion, name resolution, type checking and borrow checking afterwards.

For a capitalized, nongeneric function returning `Element`, generate a hygienic
companion macro in the same module and with the same visibility. It constructs
the function's named props struct. Rust resolves the function and macro through
the same imports, including aliases; the props name is not guessed from the
component name. Component expressions evaluate inputs before binding generated
temporaries, so those names cannot capture user expressions. Macro scaffolding
uses original spans; virtual lexer inputs never become files in the manifest.

Both executables compile this same source. The playground supplies React's
metadata and collects `.jsx` outputs alongside `.js` outputs. No dynamic
procedural macro loading or intermediate Rust source file is involved.

## Why

The syntax layer has one responsibility: translate markup into typed Rust.
React semantics, effect ordering and output generation stay in their existing
layers. Original expression spans survive into THIR and source maps. The
formatter also carries a coalesced mapping onto an opening tag when that tag
moves from a `return` line onto its own line.

## Alternatives

- Procedural macro: conventional on native Rust, but dynamic macro loading is
  unavailable in this WASM compiler; a separate browser implementation would
  duplicate the parser.
- Source-text preprocessing: simpler at first, but introduces generated Rust
  files and a second source-map composition step.
- Builder-only syntax: remains supported, but does not meet the markup goal.

## Consequences

This is a deliberately bounded compiler extension, not full JavaScript JSX.
Text must be quoted, expressions are Rust, and JSX nested inside other macros
is not expanded. Stock rust-analyzer does not know this extension. Components
with generic props, memo and context remain expressible through typed bindings.
Named component props cannot be mixed with a spread: use an explicit Rust
struct update. The syntax reserves `jsx!` and a component's name in the macro
namespace. See the [syntax guide](../jsx.md) for the complete current boundary.

Tests cover runtime rendering, evaluation order, props errors, module loading,
original source lines, native/WASM output parity, browser compilation and state
preservation through actual Vite edits in Chromium.
