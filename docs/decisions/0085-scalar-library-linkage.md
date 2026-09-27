# 0085. An experimental scalar library contract links separately compiled crates

Status: Accepted. Extends 0019 and 0084 for a deliberately narrow subset.

## Context

Sharing a source module does not prove dependency linkage. A Cargo path library
must be compiled separately, remain checked by rustc, and expose ordinary ES
module exports that a consumer can import without copying its implementation.

## Decision

`--library --manifest <path>` adds a version-1 `library` contract to the existing
build manifest. It lists public free functions with no generic parameters or
async return, accepting `bool`, `i32`, or `u32`, and returning one of those types
or unit. Other signatures are omitted. Their use through this contract is an
error, not an approximation. Exported methods, aggregates, traits, generic
functions and serialization evidence are outside this ABI.

A consumer supplies `--dependency <manifest>` for each linked library, alongside
the real rustc `--extern alias=<metadata>` flags. Rustc still checks the consumer
against that metadata. The read-only lowering adapter matches the dependency's
canonical crate/function identity and scalar signature, then uses existing import
name allocation and operand sequencing. Function values work as well as calls.
Relative imports are relocated by output planning, as other relative JS imports
already are. Cargo aliases do not change the canonical crate identity.

The owned contract and filesystem validation live in `src/library.rs`; rustc
identity/signature translation lives in `src/lower/library.rs`. Downstream linking
and printing remain independent of rustc. Dependency manifests, artifacts and
sources become consumer input dependencies, never consumer-owned outputs.

Before compiling, reject mismatching compiler identities, unsupported library ABI
versions, duplicate crate names, changed artifact/input fingerprints, missing
exports and signature mismatches. Publication retains its existing failure rules.
Library inputs are fingerprinted when the producer plans its output. A source
edit therefore requires rebuilding its library before rebuilding the consumer.
These fingerprints detect accidental staleness, not malicious tampering.

## Why

Scalars have an explicit, already-tested representation shared by both modules.
This establishes a real linkage path without pretending that trait dictionaries,
generic specialization or arbitrary layouts already have a stable crate ABI.
Generated JavaScript uses named imports and ordinary function calls.

## Alternatives

Copying dependency source into the consumer would evade separate compilation.
Defining an unrestricted ABI now would promise representations and specialization
rules that the compiler does not yet provide.

## Consequences and limits

`test/cargo-link.test.ts` resolves an independent Cargo workspace, obtains real
metadata through pinned Cargo, compiles the library and consumer separately, and
compares native and JS results before and after a dependency edit. It covers an
alias, nested consumer modules, function values, sibling output paths, unsupported
signatures and rejection without overwriting prior output.

The test orchestrates these commands explicitly. `planCargoLibraries` remains a
planner; the build adapter and Vite do not yet compile its graph. No compiled-crate
cache or automatic rebuild scheduler is introduced. Callers must pair native
metadata and JS from the same source, features, target and compiler build. The
contract checks signatures and source/artifact fingerprints, but does not bind
the native metadata bytes, Cargo flags or compiler executable bytes. The existing
release identity alone cannot distinguish two development compiler builds.

Same-named crate versions are rejected. This proof has no transitive Cargo build
coverage, registry dependencies, build scripts, procedural macro orchestration,
shared runtime state, generic ABI, or browser dependency-provisioning guarantee.
Those remain M8 work; this is not a stable general-purpose crate ABI.
