# 0106. A trait's type parameters, associated types and generic methods

Status: Accepted in part: a trait's type parameters and associated types. A
trait's generic methods, and generic associated types, are to come. Extends [0049](0049-traits-and-generics.md)
and [0051](0051-generic-options.md).

## Context

ADR 0049 supports the crate's own traits without type parameters of their
own. rustc's tests stop at the rest more than at anything else about traits:

```
119  generic trait parameters     trait Convert<T> { fn convert(&self) -> T; }
109  associated types             trait Source { type Item; }
 20  generic trait methods        trait Shape { fn f<T>(&self, t: T); }
```

and one kind of test often has all three. Most of what a generic trait needs
was there already, for std's: `impl From<f64> for Meters` is a dictionary
of its own, `metersFromF64`, and a function's evidence is named apart.

## Decision

**A trait's type parameters are its dictionary's, as std's are.**

- **Each impl is a dictionary of its own**, named for the trait's arguments
  too: `metersConvertF64` and `metersConvertString` of two impls of
  `Convert<T>` for `Meters`. A function bounded by both is given both,
  `both(x, XConvertF64, XConvertString)`.
- **A `dyn` of one is a pair** (ADR 0049), its dictionary the impl's for its
  arguments: `&dyn Convert<String>`.
- **A supertrait's key is its trait's name**, `PartialEq`, or with the
  arguments the trait declares for it where it has two of one trait:
  `trait Both: Label<u32> + Label<String>` has `LabelU32` and `LabelString`.
  The key is the declaration's, not an impl's arguments', so a generic
  impl's dictionary and its caller agree: `Pair<A, B>: Label<A> + Label<B>`
  is `LabelA` and `LabelB` for `impl Pair<u32, String>` too.
- **A higher-ranked supertrait, `for<'a> Greet<&'a str>`, is one
  dictionary**, its lifetime erased, as a higher-ranked bound is: rustc's
  trait selection takes none that's bound.
- **An `Option<T>` of a trait's `T` is ADR 0051's:** boxed where its payload
  could look like `None`, which `$some` and `$pop` box and nothing else
  does. So a concrete impl's `Option<u32>` and a generic impl's `Option<T>`
  are one representation, and generic code tells a taken `None` from none
  left: `count(&mut Stack { list: vec![None, None, Some(1)] })` is 3.

**An associated type is a type only a caller knows, as a type parameter
is:** `<S as Source>::Item` in generic code, and std's, `I::Item` of an
`Iterator`. Where rustc can say what it is, `<Count as Source>::Item` in
an impl's signature, it's that type.

- **What a type parameter's dictionaries are for, its are:** showing,
  comparing, cloning and defaulting one, and boxing an `Option` of it
  (ADR 0051). A bound on one is evidence as a type parameter's is,
  `shown(s, SSource, SItemDebug)` of `where S::Item: Debug`, given of the
  type rustc makes it for the caller, `String`'s `Debug`.
- **A bound the trait declares on one, `type Label: Display`, is in the
  trait's dictionary**, as a supertrait is, keyed by the type's name and the
  bound's, `LabelDisplay`: rustc proves `<L as Labeled>::Label: Display`
  from the trait, so generic code finds it in `L`'s `Labeled`.
- **A `dyn` of a trait with one says what it is**, `dyn Source<Item = u32>`.
- **A value of one has nothing to drop only where nothing does**, the
  crate's own types and a library's: no drop function is given for one, as
  a type parameter's is (ADR 0098). Elsewhere it's an error.
- **Not one with parameters of its own**, `type Item<'a>`: an error.

**A trait's generic method is given its own evidence where it's called**,
after the dictionary's: this is to come.

## Why

- **A dictionary per impl is what rustc resolves:** `x.convert()` of a
  `Convert<String>` is that impl's method, and generic code is given the
  dictionary its bound names.
- **Naming a supertrait by its declaration** is the one name both sides of
  a generic call can compute: the impl knows its arguments, and generic
  code its own parameters.

## Consequences

- **A trait's type parameters are in** (`generic_traits`): two impls of one
  trait, a generic impl, a default method, a nullish payload through
  generic code, a `dyn`, and supertraits, of one trait twice, generic, and
  higher-ranked. Of the 119 rustc tests stopping at a trait's parameters,
  50 pass, none giving another answer; 26 stop at associated types and 15
  at generic methods. Found by them: two supertraits of one trait collided,
  and a higher-ranked one crashed rustc's trait selection.
- **Associated types are in** (`associated_types`): the crate's, a
  function's `Vec<S::Item>` for two impls, an equality bound, a `where`
  bound, a bound the trait declares, std's `I::Item`, and a `dyn` of one;
  and the diagnostics test's refusals, of a value with a destructor and a
  generic associated type. Of the 109 rustc tests stopping at associated
  types, 61 pass; of all 1,139 known failures, 129, none giving another
  answer. Found by them, as they got further: rustc's instance resolution
  panics on arguments it can't normalize, so they're normalized first, or
  not resolved; and an `extern` declaration of the crate's own
  `#[no_mangle]` function was a JS global no JS has, which is refused.
- A trait's default method copied into a generic impl, `impl<T> .. for
  Stack<T>`, still stops at ADR 0098's destructors: `T` might have one.
