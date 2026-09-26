# 0075: JSX is the public syntax for React elements

## Decision

Use `jsx!` to construct every React element. Builder functions and methods remain
compiler plumbing, hidden from the API documentation. A resolved-HIR check rejects
handwritten calls and function references, including aliases, unused functions,
closures and calls inside JSX expressions. Object APIs such as `Style` and React
DOM options are unaffected.

The syntax pass marks its generated calls with an internal `rust_js::jsx`
expression attribute. It marks each call separately, so original Rust expressions
inside props and children receive no exemption. This attribute is compiler
metadata, not an application API.

Component props constructors remain hygienic macros in the component's module.
Generic functions infer their type arguments; explicit arguments use Rust's
`::<T>` syntax. Typed `thread_local!` declarations of memoized, lazy, forwarded-ref
and context values get constructors too. Contexts support `.Provider` for React
18+, and the context itself for React 19+. Other component values can use a typed
props spread, including `()` for a value with no props.

All supported built-in components have JSX spellings. Ref objects and callbacks
use `ref`; URL, function and dispatch actions use `action` or `formAction`. Marker
traits distinguish their Rust types while retaining React's version gates.

## Why

Two public ways to construct the same UI impose a choice with no benefit to the
application. JSX provides the familiar spelling; typed bindings still let rustc
check it without a second type system. This replaces ADR 0072's decision to offer
builders alongside JSX. It does not change generated JavaScript, source mapping,
module boundaries or Fast Refresh.

## Cost

This is a source-breaking change for builder users. All maintained examples and
fixtures use JSX. A syntax pass cannot resolve arbitrary Rust type aliases before
type checking: named-prop constructors recognize function declarations and the
explicit wrapper types above; other component values use props spreads.
