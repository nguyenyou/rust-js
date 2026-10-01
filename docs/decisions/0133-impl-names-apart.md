# 0133. Impls whose names would be the same are named by their arguments

Status: Accepted. Extends [0049](0049-traits-and-generics.md).

## Context

A trait impl's JS name is its type's and its trait's, `circleShape` (ADR
0049), and its methods' are after it, `circleShape_area`. Two impls of one
trait could get one name, which was an error, "put the implementations in
separate modules":

- for one type with other arguments: `impl Describe for Vec<i32>` and
  `impl Describe for Vec<String>`, both `vecDescribe`;
- for a type with other trait arguments that are references:
  `impl Convert<u8> for ()` and `impl Convert<&u8> for ()`;
- for two types of one name, local to two functions: a `struct Local` in
  each, both `localDescribe`.

## Decision

**Where impls of one trait in the crate would have one name, each is named
with its arguments, its type's and its trait's, and, where those are the
same too, numbered in the order they're declared:**

| Rust | JS |
|---|---|
| `impl Describe for Vec<i32>`, `Vec<String>` | `vecI32Describe`, `vecStringDescribe` |
| `impl Describe for Wrapper<&str>`, `Wrapper<u8>` | `wrapperRefStrDescribe`, `wrapperU8Describe` |
| `impl Convert<u8> for ()`, `Convert<&u8>` | `__ConvertU8`, `__ConvertRefU8` |
| a `Local` in `first()`, and one in `second()` | `localDescribe`, `localDescribe2` |

- **An argument that's its parameter's default isn't named**: `Vec<i32>`
  is `VecI32`, not with its allocator.
- **An impl whose name no other one has keeps its short name**, so a crate
  without two alike gets the JS it gets today.
- **The name is found from the crate's impls of the trait**, the same
  wherever it's asked for, so its accessor and its methods agree.

## Why

- **It's the JS a person writes**: two things of one name are told apart
  by what they're for, and only where they must be.
- **It compiles what was an error**, which it was only for its names.

## Consequences

- The collisions compile, compared with native Rust by the `impl_names`
  corpus case.
