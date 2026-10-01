# 0119. Bindings the community maintains, in one repository

Status: Accepted in part: the repository, `rust-js-lang/bindings`, its
`AGENTS.md`, and its first binding, `@rust-js-bindings/canvas-confetti`
1.9.0, its example run as its test. Its bot, its publishing as it merges,
and the following of each library are deferred: a binding is published by
hand, as rust-js's packages are (ADR 0120). Builds on
[0116](0116-binding-versions.md) and [0118](0118-bindings-on-npm-only.md).
A checker of bindings against their libraries is designed here, and
deferred.

## Context

A program can bind any JS library itself, `#[rust_js::link_name]` in its
own code (ADRs 0021, 0039): rust-js can't bind all of npm, so it must.
But a binding is a claim about another project's API, and a wrong one
compiles, then fails in the browser, `toast is not a function`. Each app
that binds a library itself makes the claim again, and keeps it right as
the library moves, or doesn't: a coding agent writes one faster, and as
wrong. A binding kept once is right, or fixed, for every app that uses it.

Others keep them so:

- **DefinitelyTyped** (its README, read September 2026): one repository, a
  directory for each library, `types/<library>/`, with a `package.json`
  whose `owners` the bot asks to review each change to it; a change an
  owner approves, with its tests, merges soonest, and each is reviewed by
  a maintainer of the repository too, who alone admits a new package or a
  new dependency, "to make sure that `@types` packages don't depend on
  malicious packages". Only a library "actually in use today" is admitted,
  and a PR "must" be for its author's use; a coding agent may send one,
  never more, marked `[auto-generated]`. `main` is published to `@types/*`
  within an hour; a package's version is its library's major and minor,
  and its own patch, `19.3.9999` in its `package.json`, so a fix is its
  next patch. A library that comes to ship its own types has its package
  deprecated, `not-needed`. Only TypeScript releases under two years old
  are tested. Its weakness is its own README's: "type declaration package
  updates lag behind library updates", as users, not the library's
  maintainers, update them. And its tests type-check the declarations
  against themselves, never against the library.
- **ReScript**: no repository; a binding is anyone's npm package, found by
  keyword, and its generated web bindings are kept by hand since, as
  generating them again would undo the hand's work.

## Decision

**The bindings the community keeps are one repository's,
`rust-js-lang/bindings`, as DefinitelyTyped's are: a directory for each
library, owners for each, a bot that merges what an owner approves and CI
passes, and each published to npm (ADR 0118) as its next patch, as it
merges.** A binding is kept right over time, by hand: a mistake a user
meets is a fix, released within the hour.

```
rust-js-lang/bindings
├─ bindings/canvas-confetti/
│  ├─ Cargo.toml       rust-js-bindings-canvas-confetti 1.9.x: its library's major and minor (ADR 0116)
│  ├─ package.json     owners, and its peer dependencies (ADR 0118)
│  ├─ src/lib.rs       written, by a person or a coding agent
│  └─ README.md
├─ examples/canvas-confetti/   an app that installs it, as a user's does
├─ test/canvas-confetti.test.ts the example, built by Vite and run in a browser
├─ AGENTS.md           how a binding is written
└─ tools/              the bot, the publisher: deferred
```

### How a binding is written

**By hand, as React's is, most often a coding agent's**: it reads the
library's `.d.ts`, its documentation and its source, and writes the
binding and its examples. Nothing generates bindings (ADR 0116): what
TypeScript says has no one Rust for it, and an agent reads what a `.d.ts`
leaves out, in the documentation and the source.

- **`AGENTS.md` says how**, for an agent and a person alike: what to read
  of the library, how each of its constructs is bound, as the rust-js
  documentation has them, and what examples to write.
- **Its examples are its tests**: compiled by rust-js and run against the
  library, as rust-js's own corpus is, in a headless browser for a library
  that needs a page: `bun run test`, before a PR, and in CI, deferred.

### Kept right over time

**A binding is right as far as its examples reach, and made more right as
it's used.** What it gets wrong beyond them, a name, a type, what may be
`null`, compiles, and a user meets it:

- **A user's report is a fix**, with an example that shows it, as its
  test; an owner's review, and the bot merges it.
- **A fix is published as it merges**, the binding's next patch, within the
  hour, and an app has it with its package manager's next install.
- **A fix stays fixed**: its example stays, and runs against each release
  of the library after.

### What's admitted

- **A binding for a library someone uses**, as DefinitelyTyped admits
  them: a PR is its author's, a person who uses the library, whoever wrote
  it. One a coding agent wrote says so, and none is one of a batch for
  libraries nobody asked for. An owner reviews what an agent wrote, as
  what a person did.
- **Declarations, not code that runs as it's built**: no build script, no
  procedural macro. A binding is in every build of every app that uses it.
- **What it depends on is `js`, `webapi`, `@rust-js/build`, other bindings
  of the repository, and its library**, each a peer dependency (ADR 0118);
  another, and each new one, is a maintainer's to admit.

### Who keeps what

- **A binding's owners**, in its `package.json`'s `owners`, as
  DefinitelyTyped's: the bot asks them to review each change, and a change
  an owner approves, whose CI passes, the bot merges.
- **The maintainers**, rust-js's, admit a binding, an owner, and a
  dependency, review a change that breaks a program within a minor, and
  keep a binding no owner keeps. One with no owner, whose examples fail
  for three months, is deprecated.

### A library's new release

Deferred, with the bot.

**The bot follows each library bound**, where DefinitelyTyped waits for a
user: a new minor release on npm is an issue its owners are asked to act
on, with its changelog, and its examples' run against it. A coding agent,
given the issue, the release's `.d.ts` and its changelog, drafts the
binding for that minor, a PR the owners review, as any.

### Published

Each by hand for now, by a maintainer, as rust-js's packages are (ADR
0120). Publishing as it merges, with provenance, the two manifests written
from one, and the testing with a year of rust-js's releases are deferred,
with the bot: a binding's `package.json` and `Cargo.toml` are written by
hand, as `AGENTS.md` says.

- **`main`, as it merges, to npm**: each binding changed, as its next
  patch, the publisher's to number, as DefinitelyTyped's `.9999` is; with
  npm's provenance, which says which commit of the repository built it.
- **Named `@rust-js-bindings/<library>`**, an npm organization of its own,
  as DefinitelyTyped's `@types/` is TypeScript's, so the bot that publishes
  it can't publish rust-js's `@rust-js/*`, and a library named `build`
  isn't rust-js's `@rust-js/build`. A scoped library's is as
  DefinitelyTyped writes it, `@babel/core`'s `@rust-js-bindings/babel__core`.
  The crate is `rust-js-bindings-<library>`, its library's name, `sonner`.
- **The two manifests from one**: the publisher writes the peer
  dependencies on bindings, `js` and `webapi` from what `Cargo.toml` asks,
  so they agree (ADR 0118).
- **Deprecated when the library binds itself**, a rust-js binding of its
  own in its package, as DefinitelyTyped's `not-needed`.
- **Tested with rust-js's releases of the last year**: as its attributes
  only grow (ADR 0116), a binding keeps compiling with each after the one
  it needs.

### What's rust-js's own

`js` and `webapi` are released with rust-js, from its repository (ADR
0116): the standard library every binding shares. So is `react`, as
`@rust-js/react`, at the compiler's version (ADR 0120): its build script
gates each API by the app's React (ADR 0043), and rust-js's tests of
`jsx!` are its. The repository's first binding is canvas-confetti's.

### A checker, later

**What a binding claims can be checked against the library**, where
DefinitelyTyped's declarations are checked only against themselves: a
binding names what the library must have, and the library is on npm to
load. It's deferred: bindings are kept, and fixed, by hand first, and it
comes when their number, or their mistakes, call for it. Designed:

- **rust-js lists the claims**, from what `src/lower/bindings.rs` reads of
  each `#[link_name]`: a function, a `new`, a method or a getter, its
  arguments and what it returns, as JS has them, and its Rust line.
- **① What it names is there**: the library loaded, with its install
  scripts not run, at the lowest and highest version of its range, and
  each name found, a function, a class, a method on its class's
  prototype, a getter. Which class a Rust type is, the checker learns from
  a `new`, or from the library's `.d.ts`.
- **② What it says agrees with the library's types**: each claim written
  as a TypeScript assignment, the library's item to the binding's, which
  `tsc` accepts only if the library's can stand in for it, `null` and all.
  It needs how rust-js represents each Rust type in JS, `Option` first,
  written down exactly.
- **③ Its examples**, as they are.
- **Findings at the Rust line**, `file:line:col: ..`, as ADR 0117's checks
  are read, so the same checker is a program's own, for the bindings it
  wrote itself.

## Why

- **One binding, fixed once, for every app**: an app that binds a library
  itself stays the escape hatch, and the repository is where a binding
  many need is kept right.
- **By hand, and over time, is how a binding gets right**: a fix is a line,
  and a user's report says which; an example keeps it fixed. What checks
  would catch before a user does comes later, with the checker.
- **DefinitelyTyped's way is proven, at its scale**: owners who review
  their own libraries' changes, a bot that merges theirs, maintainers who
  admit what could harm, and a publisher that numbers releases.
- **Following the library, not waiting for a user**, is DefinitelyTyped's
  own lag, handled by a machine.

## Alternatives

- **Each binding its author's repository, ReScript's way**: no owner but
  its author, and a binding left behind as its author moves on, found by a
  keyword.
- **Only rust-js's maintainers' bindings**: kept right, and too few; a
  library they don't bind is each app's again.
- **Generated from `.d.ts`, ScalablyTyped's way**: every library, and
  every binding only as right as its `.d.ts`, which a generator can't
  read past, where an agent reads the documentation and the source; and
  TypeScript's overloads, unions and mapped types have no one Rust for a
  generator to choose. ReScript's generated web bindings needed a hand,
  and generating them again would have undone it.
- **The checker first**: mistakes caught before a user meets one, and
  the most work of all this before a first binding is published.

## Consequences

- **A mistake is met before it's fixed**: by a user, in the browser, as a
  binding's examples didn't reach it. The fix is fast; the first meeting
  isn't prevented.
- **Examples are what keeps a binding right**: each binding has them, and
  each fix adds one.
- **Running examples runs the library's code**: in CI, in a sandbox of its
  own, a library's install scripts not run.
- **Proven**: canvas-confetti's binding in the repository, its example
  firing the real library's confetti in a browser, published by hand to
  `@rust-js-bindings`, and used by an app from npm through ADR 0118's
  patch.
- **Deferred**: the bot, its owners' reviews and its issue for a
  library's new minor; publishing as it merges.

## Unresolved

- Who maintains the repository with rust-js's maintainers, before it
  admits the community's bindings.
- `AGENTS.md`: how each of a `.d.ts`'s constructs is bound, which the
  rust-js documentation of bindings is to say first.
