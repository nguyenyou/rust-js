# 0106. A trait's type parameters, associated types, generic methods and constants

Status: Accepted: a trait's type parameters, associated types, generic
methods and constants. Generic associated types, and a generic impl's
constant of its parameters, are to come. Extends [0049](0049-traits-and-generics.md)
and [0051](0051-generic-options.md).

## Context

ADR 0049 supports the crate's own traits without type parameters of their
own. rustc's tests stop at the rest more than at anything else about traits:

```
119  generic trait parameters     trait Convert<T> { fn convert(&self) -> T; }
109  associated types             trait Source { type Item; }
 30  associated constants         trait Shape { const SIDES: u32; }
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
- **A field of one is the type it stands for where its struct is used:**
  `Node<i32>`'s `value: K::Value` is an `Option<u32>`, as rustc's own
  types of places are. A field's type is normalized where it has one, so
  the struct's shape, what an `Option` of it is, and its clones and drops
  are the type's own; in generic code it's still a caller's.
  (Amended: a value of such a struct was an error, `<i32 as Key>::Value`,
  in 17 rustc tests.)
- **A value of one has nothing to drop only where nothing does**, the
  crate's own types and a library's: no drop function is given for one, as
  a type parameter's is (ADR 0098). Elsewhere it's an error.
- **One with lifetimes of its own**, `type Item<'a>`, is one too (ADR 0146).
  (Amended: it was an error.)

**A trait's generic method is given its own evidence where it's called,**
`describe<T: Display>`'s `TDisplay`:

- **Called where the impl is known, it's the impl's method**, given all of
  its evidence, as any generic function is.
- **Called through a dictionary, its own evidence comes after its
  arguments, in the order the trait declares it**: `SShape.describe(s, 7,
  { fmt: String })`. The dictionary's entry passes each on as the impl's
  bound it is, which may be in another order, `Square`'s `A: Debug +
  Clone`, with the impl's own evidence, made with the dictionary; and one
  that passes on just what it's given, in order, is the method itself.
- **A default copied into an impl takes it too**, after its arguments.
- **Where a type may have a destructor, it's an error:** called through a
  dictionary, it's given no drop function for its own type parameters, as
  an associated type isn't.

**A trait's constant is its value where the type is known, and its
dictionary's in generic code:**

- **Where rustc can say which impl's it is**, `Square::SIDES` or
  `<Square as Shape>::NAME`, it's the value rustc computed, written in
  place, as a type's own constant is (ADR 0031): `4`, or `"shape"` of a
  default.
- **In generic code, `S::SIDES` is `SShape.SIDES`**: each impl's dictionary
  has its value, the impl's or the trait's default, as rustc computed it
  for that impl, `{ SIDES: 4, NAME: "square", area: .. }`.
- **Of a type changed in place, it's a getter**, `get ZERO() { return { n:
  0 }; }`: each use is a value of its own, as a Rust constant's is, so a
  `bump` of one doesn't change the next. Of any other type, a value.
- **A dictionary has only the constants generic code reads**, or all, of a
  library, whose consumers may (ADR 0100): rustc evaluates only the
  constants a program uses, and a default no one reads may not evaluate,
  `360 / Self::SIDES` of a `Wrap` with none.
- **Not yet: a generic impl's constant of its parameters**, `impl<T: Foo>
  Foo for Proxy<T> { const X: i32 = T::X; }`, which rustc can't compute
  for every `T` at once. An error, as are a constant with parameters of
  its own, `const SIZE<T>`, and a `type const`.

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
- **Generic methods are in** (`generic_trait_methods`): a default, one
  bounded by a closure, one with two bounds an impl orders the other way,
  called on a known type and from generic code, and the diagnostics test's
  refusal where a type has a destructor. Of the 20 rustc tests stopping at
  them, 11 pass; of all 1,139 known failures, 154, none giving another
  answer.
- **Associated constants are in** (`associated_consts`): known and generic
  uses, a default, a generic impl's constant that doesn't need its
  parameter, one of a type changed in place, and a default that would divide
  by zero, unread; and the diagnostics test's refusals. Of the 30 rustc tests
  stopping at them, 18 pass, none giving another answer. Found by them:
  evaluating every constant for a dictionary made rustc report a default's
  cycle and overflow that nothing used, and a `type const` crashed rustc's
  type checking.
- A trait's default method copied into a generic impl, `impl<T> .. for
  Stack<T>`, still stops at ADR 0098's destructors: `T` might have one.
