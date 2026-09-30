// Mutations of the compiler, each a bug it had, or one a rule of it keeps
// out, put back: the tests named for each must fail against a compiler
// built with it, and pass against the compiler as it is (ADR 0093). A
// mutation that no longer applies, doesn't build, or that its tests don't
// catch fails the run, so the tests are shown to see what they're for.
//
//   bun scripts/mutations.ts                  # every mutation
//   bun scripts/mutations.ts copy-on-read ..  # the ones named
//
// Each builds natively a few test programs, which macOS makes slow; the
// rustc tests workflow runs them all on Linux with `mutations`.

import { chmodSync, cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { runSync, stopped, type Exit } from "../test/child";

const root = join(import.meta.dir, "..");

export type Mutation = {
  name: string;
  /** What it breaks, as Rust would see it. */
  breaks: string;
  file: string;
  find: string;
  replace: string;
  /** `bun test` arguments whose tests must catch it. */
  tests: string[];
};

export const mutations: Mutation[] = [
  {
    name: "conditional-prerequisites",
    breaks: "unselected branches and failed match patterns still run their calls and copy-back",
    file: "src/lower.rs",
    find: "        if yes.statements.is_empty() && no.statements.is_empty() {",
    replace: "        out.append(&mut yes.statements);\n        out.append(&mut no.statements);\n        if true {",
    tests: ["test/corpus.test.ts", "-t", "conditional_regions"],
  },
  {
    name: "element-value-first",
    breaks: "`v[i] = f()` checks `i` before `f` runs, and reads what the index wrote",
    file: "src/lower/places.rs",
    find: "        if (writes && !value.is_constant()) || (panics && value.has_effects()) {",
    replace: "        if false && ((writes && !value.is_constant()) || (panics && value.has_effects())) {",
    tests: ["test/corpus.test.ts", "-t", "assignment_order"],
  },
  {
    name: "compound-place-read",
    breaks: "`x += g()` reads `x` before `g`, which changes it, runs",
    file: "src/lower/places.rs",
    find: "let value = if read && value.has_effects() && self.may_change(lhs) {",
    replace: "let value = if false && read && value.has_effects() && self.may_change(lhs) {",
    tests: ["test/corpus.test.ts", "-t", "assignment_order\\.rs"],
  },
  {
    name: "i32-wrap",
    breaks: "`i32` arithmetic doesn't wrap at 32 bits",
    file: "src/lower/representation.rs",
    find: "            Num::I32 => Expr::bin(Op::BitOr, e, Expr::num(0)),",
    replace: "            Num::I32 => e,",
    tests: ["test/corpus.test.ts", "-t", "wrapping\\.rs"],
  },
  {
    name: "u64-wrap",
    breaks: "`u64` arithmetic doesn't wrap at 64 bits",
    file: "src/lower/representation.rs",
    find: '            Num::U64 => as_n("asUintN"),',
    replace: "            Num::U64 => e,",
    tests: ["test/corpus.test.ts", "-t", "wrapping\\.rs"],
  },
  {
    name: "index-panic-message",
    breaks: "an index out of bounds panics with another message than Rust's",
    file: "src/runtime.rs",
    find: String.raw`function $index(items, index) {\n  if (index < 0 || index >= items.length) throw new Error(` + "`index out of bounds: the len is",
    replace: String.raw`function $index(items, index) {\n  if (index < 0 || index >= items.length) throw new Error(` + "`index out of bounds: the length is",
    tests: ["test/corpus.test.ts", "-t", "index_out_of_bounds"],
  },
  {
    name: "copy-on-read",
    breaks: "a `Copy` value read from a place is that place, not a copy",
    file: "src/lower/representation.rs",
    find: "        if self.contains_mutated(ty) && self.is_copy(ty) {",
    replace: "        if false && self.contains_mutated(ty) && self.is_copy(ty) {",
    tests: ["test/corpus.test.ts", "-t", "copy_mutation"],
  },
  {
    name: "guard-statements",
    breaks: "a guard's statements don't run before its test",
    file: "src/lower/patterns.rs",
    find: "                before.push(StmtKind::If(guard, body, None).at(span));",
    replace: "                before.clear();\n                before.push(StmtKind::If(guard, body, None).at(span));",
    tests: ["test/corpus.test.ts", "-t", "guard_statements"],
  },
  {
    name: "crash-after-rejection",
    breaks: "rust-js panics after it says what it doesn't support",
    file: "src/lower.rs",
    find: '        self.tcx\n            .dcx()\n            .span_err(span, format!("rust-js does not support {what} yet"))\n    }',
    replace: '        self.tcx.dcx().span_err(span, format!("rust-js does not support {what} yet"));\n        panic!("a crash after the rejection")\n    }',
    tests: ["test/corpus.test.ts", "-t", "union_const|closure_clone\\.rs"],
  },
  {
    name: "operand-capture",
    breaks: "an earlier operand runs after a later one's statements",
    file: "src/lower.rs",
    find: "            if !evaluated.statements.is_empty() {",
    replace: "            if false {",
    tests: ["test/semantics.test.ts", "-t", "operand_prerequisites"],
  },
  {
    name: "union-field",
    breaks: "a union's field is read as a struct's, and rust-js panics",
    file: "src/lower.rs",
    find: "    ty.ty_adt_def().is_some_and(|adt| adt.is_union())",
    replace: "    ty.ty_adt_def().is_some_and(|adt| adt.is_union() && false)",
    tests: ["test/corpus.test.ts", "-t", "union_const"],
  },
  {
    name: "dyn-bound-lifetimes",
    breaks: "a `dyn for<'a>` trait's dictionary is asked of rustc with its lifetime bound, and rustc panics",
    file: "src/lower/traits.rs",
    find: "                self.tcx\n                    .instantiate_bound_regions_with_erased(p.with_self_ty(self.tcx, self_ty))",
    replace: "                p.with_self_ty(self.tcx, self_ty).skip_binder()",
    tests: ["test/corpus.test.ts", "-t", "higher_ranked_dyn"],
  },
  {
    name: "begin-panic-payload",
    breaks: "`panic!(5)` before edition 2021 throws a message Rust never shows",
    file: "src/lower/calls.rs",
    find: "                if !text {",
    replace: "                if false && !text {",
    tests: ["test/corpus.test.ts", "-t", "begin_panic_value"],
  },
  {
    name: "lazy-rhs-statements",
    breaks: "`a || f(&mut y)` runs the statements `f`'s call needs whether or not `a` decides",
    file: "src/lower.rs",
    find: "                    if rhs_out.is_empty() {\n                        return Ok(Expr::bin(js_op, l, r));",
    replace: "                    if true {\n                        out.extend(rhs_out);\n                        return Ok(Expr::bin(js_op, l, r));",
    tests: ["test/corpus.test.ts", "-t", "lazy_effects"],
  },
  {
    name: "while-condition-statements",
    breaks: "a `while` condition's statements are put in the loop, after its test",
    file: "src/lower/loops.rs",
    find: "                if before.is_empty() {\n                    self.stmt(then, &Dest::Discard, &mut body_out)?;",
    replace: "                if true {\n                    body_out.extend(before);\n                    self.stmt(then, &Dest::Discard, &mut body_out)?;",
    tests: ["test/corpus.test.ts", "-t", "lazy_effects"],
  },
  {
    name: "at-binding-copy",
    breaks: "a binding after `@` reads its part of the value in place, which the binding before it changes",
    file: "src/lower/patterns.rs",
    find: "        let stable = stable && !(bindings.len() > 1 && bindings.iter().any(|b| b.whole));",
    replace: "        let stable = stable || bindings.iter().any(|b| b.whole);",
    tests: ["test/corpus.test.ts", "-t", "binding_after_at"],
  },
  {
    name: "option-some-rest",
    breaks: "`Some(..)`, whose `..` names no field, is taken as `None`",
    file: "src/lower/patterns.rs",
    find: "                    let op = if some { Op::LooseNe } else { Op::LooseEq };",
    replace: "                    let op = if some && false { Op::LooseNe } else { Op::LooseEq };",
    tests: ["test/corpus.test.ts", "-t", "option_rest_pattern"],
  },
  {
    name: "size-align-swap",
    breaks: "`align_of` is the type's size",
    file: "src/lower/calls.rs",
    find: "                let bytes = if matches!(known, Std::AlignOf) {\n                    layout.align.abi.bytes()",
    replace: "                let bytes = if matches!(known, Std::AlignOf) {\n                    layout.size.bytes()",
    tests: ["test/corpus.test.ts", "-t", "size_of\\.rs"],
  },
  {
    name: "array-repeat-shared",
    breaks: "`[x; N]` of what's changed is one object, `N` times",
    file: "src/lower.rs",
    find: "                let copied = if self.is_copy(item_ty) {\n                    self.contains_mutated(item_ty)",
    replace: "                let copied = if self.is_copy(item_ty) {\n                    false",
    tests: ["test/corpus.test.ts", "-t", "array_repeat"],
  },
  {
    name: "never-loop-value",
    breaks: "a `loop` that never ends, used as a value, is rejected",
    file: "src/lower/body_queries.rs",
    find: "            ExprKind::NeverToAny { source } => match self.thir[source].kind {\n                ExprKind::Loop { body } => Some(body),",
    replace: "            ExprKind::NeverToAny { source } => match self.thir[source].kind {\n                ExprKind::Loop { body } if false => Some(body),",
    tests: ["test/corpus.test.ts", "-t", "loop_values"],
  },
  {
    name: "static-struct-variant",
    breaks: "a static struct's fields are read one place along, after a variant index only an enum has",
    file: "src/lower/representation.rs",
    find: "            let variant = parts.variant.filter(|_| ty.is_enum()).map(|v| {",
    replace: "            let variant = parts.variant.map(|v| {",
    tests: ["test/corpus.test.ts", "-t", "statics"],
  },
  {
    name: "static-mut-place",
    breaks: "a `static mut` is read and written as its value, not its `{ value }`",
    file: "src/lower/places.rs",
    find: '                    true => (Expr::member(item, "value"), true),',
    replace: "                    true => (item, true),",
    tests: ["test/corpus.test.ts", "-t", "static_mut"],
  },
  {
    name: "static-mut-reference",
    breaks: "a `&mut` to a `static mut` is allowed",
    file: "src/lower/analysis.rs",
    find: '                Some(d) if tcx.is_mutable_static(d) => "`&mut` references to a `static mut`",',
    replace: '                Some(d) if tcx.is_mutable_static(d) && false => "`&mut` references to a `static mut`",',
    tests: ["test/corpus.test.ts", "-t", "static_mut_reference"],
  },
  {
    name: "atomic-fetch-new-value",
    breaks: "an atomic's `fetch_add` and the like give the new value, not the old",
    file: "src/lower/calls.rs",
    find: "                out.push(StmtKind::Assign(slot, next).at(js_span));\n                previous\n",
    replace: "                out.push(StmtKind::Assign(slot.clone(), next).at(js_span));\n                slot\n",
    tests: ["test/corpus.test.ts", "-t", "atomics"],
  },
  {
    name: "thread-local-storage-static",
    breaks: "std's storage for a `thread_local!` is taken as a static of the crate's, and rejected",
    file: "src/lower/analysis.rs",
    find: "            DefKind::Static { .. } => !tcx.is_foreign_item(d) && in_thread_local(tcx, d).is_none(),",
    replace: "            DefKind::Static { .. } => !tcx.is_foreign_item(d),",
    tests: ["test/corpus.test.ts", "-t", "thread_local_syntax"],
  },
  {
    name: "auto-trait-impl",
    breaks: "an impl of an auto trait, as `unsafe impl Sync`, is rejected",
    file: "src/lower/recognition.rs",
    find: "        || tcx.trait_is_auto(id)\n",
    replace: "",
    tests: ["test/corpus.test.ts", "-t", "marker_traits"],
  },
  {
    name: "user-deref-impl",
    breaks: "a user `Deref` is rejected",
    file: "src/lower/recognition.rs",
    find: "        LangItem::Deref,\n",
    replace: "",
    tests: ["test/corpus.test.ts", "-t", "user_deref"],
  },
  {
    name: "returned-field-write",
    breaks: "a field of what a call's `&mut` points to can't be written",
    file: "src/lower/places.rs",
    find: "                    (None, None) if self.returned(lhs) => self.referent(lhs, out)?,\n",
    replace: "",
    tests: ["test/corpus.test.ts", "-t", "user_deref|returned_references"],
  },
  {
    name: "own-pointer-unsize",
    breaks: "a pointer of the crate's own, unsized to a `dyn`, is the value it was, not a `dyn`'s",
    file: "src/lower/traits.rs",
    find: "        if let ty::Adt(adt, _) = target.kind()\n            && (adt.did().is_local() || self.krate.foreign.in_library(adt.did()))\n",
    replace: "        if let ty::Adt(adt, _) = target.kind()\n            && (adt.did().is_local() || self.krate.foreign.in_library(adt.did()))\n            && false\n",
    tests: ["test/diagnostics.test.ts", "-t", "pointer of the crate"],
  },
  {
    name: "drop-order",
    breaks: "a scope drops what it owns first first, not last first",
    file: "src/lower/drops.rs",
    find: "        for o in owned.into_iter().rev() {\n",
    replace: "        for o in owned.into_iter() {\n",
    tests: ["test/corpus.test.ts", "-t", "drop_scopes"],
  },
  {
    name: "drop-after-move",
    breaks: "a moved variable is dropped at the end of its scope too",
    file: "src/lower/drops.rs",
    find: "                None => out.push(clear),\n",
    replace: "                None => {}\n",
    tests: ["test/corpus.test.ts", "-t", "drop_scopes"],
  },
  {
    name: "drop-on-assign",
    breaks: "an assignment doesn't drop the old value",
    file: "src/lower/drops.rs",
    find: "            None => out.extend(drop),\n",
    replace: "            None => {}\n",
    tests: ["test/corpus.test.ts", "-t", "drop_scopes"],
  },
  {
    name: "drop-without-finally",
    breaks: "a scope's drops run only when it ends normally, not by `return`, `break` or a panic",
    file: "src/lower/drops.rs",
    find: "        // Nothing between the declaration and the drops: nothing to leave by.\n        if body.is_empty() {\n            out.extend(finally);\n        } else {\n            out.push(StmtKind::Try(body, finally).at(js_span));\n",
    replace: "        // Nothing between the declaration and the drops: nothing to leave by.\n        if body.is_empty() {\n            out.extend(finally);\n        } else {\n            out.extend(body);\n            out.extend(finally);\n",
    tests: ["test/corpus.test.ts", "-t", "drop_scopes|drop_on_panic"],
  },
  {
    name: "drop-move-before-operands",
    breaks: "a variable moved into a call is taken as moved before a later operand panics",
    file: "src/lower/drops.rs",
    find: "            match self.drop_state.deferred.get_mut(&(key, e)) {\n",
    replace: "            match None::<&mut Option<Stmt>> {\n",
    tests: ["test/corpus.test.ts", "-t", "drop_operand_panic"],
  },
  {
    name: "drop-function-in-branch",
    breaks: "a drop function is declared where it's first needed, a branch another call isn't in",
    file: "src/lower/drops.rs",
    find: "            made.defs\n                .push(StmtKind::Const(name.clone(), Expr::arrow(vec![param.into()], body)).at(js_span));\n",
    replace: "            out.push(StmtKind::Const(name.clone(), Expr::arrow(vec![param.into()], body)).at(js_span));\n",
    tests: ["test/corpus.test.ts", "-t", "drop_functions"],
  },
  {
    name: "drops-walk-uncached",
    breaks: "what a type drops is found again for each path to it, which takes exponential time",
    file: "src/lower/drops.rs",
    find: "            self.drop_state.cache.borrow_mut().insert(ty, found);\n",
    replace: "",
    tests: ["test/corpus.test.ts", "-t", "nested_generic_types"],
  },
  {
    name: "closure-stepped-iterators",
    breaks: "a closure's body doesn't find its own stepped iterators, and `it.next()` in one is rejected",
    file: "src/lower/bodies.rs",
    find: "            Nested::Closure { names } => {\n                self.stepped.extend(own);\n",
    replace: "            Nested::Closure { names } => {\n",
    tests: ["test/corpus.test.ts", "-t", "stepped_nested"],
  },
  {
    name: "static-mut-shared-reference",
    breaks: "a shared reference to a `static mut` is rejected",
    file: "src/lower/analysis.rs",
    find: "                ExprKind::Borrow {\n                    borrow_kind: BorrowKind::Mut { .. },\n                    arg,\n                } => (arg, false),\n",
    replace: "                ExprKind::Borrow { arg, .. } => (arg, false),\n",
    tests: ["test/corpus.test.ts", "-t", "static_mut_shared"],
  },
  {
    name: "drop-ref-parameter",
    breaks: "a parameter bound by `ref` isn't dropped as its function ends",
    file: "src/lower/bodies.rs",
    find: "                        if self.has_drops(param.ty) {\n                            self.own(*var, Expr::var(&name), param.ty, pat.span, out)?;\n",
    replace: "                        if mode.0 == ByRef::No && self.has_drops(param.ty) {\n                            self.own(*var, Expr::var(&name), param.ty, pat.span, out)?;\n",
    tests: ["test/corpus.test.ts", "-t", "drop_params"],
  },
  {
    name: "drop-let-value-context",
    breaks: "a `let`'s value is taken as moved whatever its pattern, so `let _ = x` moves `x`",
    file: "src/lower/drops.rs",
    find: "            if self.lets.contains_key(&child) {\n                return (None, child);\n            }\n",
    replace: "",
    tests: ["test/corpus.test.ts", "-t", "drop_params"],
  },
  {
    name: "drop-deref-temporary",
    breaks: "a temporary dereferenced in place, as a method call through a returned `Box`, is never dropped",
    file: "src/lower/drops.rs",
    find: "                    | ExprKind::Index { lhs: arg, .. }\n                    | ExprKind::Deref { arg },\n",
    replace: "                    | ExprKind::Index { lhs: arg, .. },\n",
    tests: ["test/corpus.test.ts", "-t", "drop_deref_temporary"],
  },
  {
    name: "temporary-without-finally",
    breaks: "a statement's temporary is dropped only when the statement ends normally, not as a panic unwinds",
    file: "src/lower/drops.rs",
    find: "            None => drop,\n        };\n        if body.is_empty() {\n            out.extend(finally);\n        } else {\n            out.push(StmtKind::Try(body, finally).at(js_span));\n",
    replace: "            None => drop,\n        };\n        if body.is_empty() {\n            out.extend(finally);\n        } else {\n            out.extend(body);\n            out.extend(finally);\n",
    tests: ["test/corpus.test.ts", "-t", "drop_temporary_operands"],
  },
  {
    name: "temporary-not-extended",
    breaks: "a temporary a `let` keeps alive is dropped at the end of the `let`, not of the block",
    file: "src/lower/drops.rs",
    find: "                    Some(scope) if Some(scope) == rest => {\n",
    replace: "                    Some(scope) if Some(scope) == rest && false => {\n",
    tests: ["test/corpus.test.ts", "-t", "drop_temporaries"],
  },
  {
    name: "unsize-array-as-dyn",
    breaks: "an array unsized to a slice is taken for a `dyn`, and rejected",
    file: "src/lower/drops.rs",
    find: "matches!(to.kind(), ty::Dynamic(..)) && self.cx.drops(from) != Drops::Nothing",
    replace: "matches!(to.kind(), ty::Dynamic(..) | ty::Slice(_)) && self.cx.drops(from) != Drops::Nothing",
    tests: ["test/corpus.test.ts", "-t", "drop_temporaries"],
  },
  {
    name: "generic-drop-not-given",
    breaks: "a generic function given a value with a destructor isn't given its drop, and never drops it",
    file: "src/lower/traits.rs",
    find: "        for &index in given {\n            drops.push(self.drop_function(args.type_at(index as usize), span)?);\n",
    replace: "        for &index in given {\n            drops.push(None);\n",
    tests: ["test/corpus.test.ts", "-t", "drop_generic"],
  },
  {
    name: "generic-drop-not-passed-on",
    breaks: "a generic function that passes its `T` to another doesn't pass its drop on",
    file: "src/lower/drops.rs",
    find: "            return Ok(drop);\n",
    replace: "            return Ok(None);\n",
    tests: ["test/corpus.test.ts", "-t", "drop_generic"],
  },
  {
    name: "part-move-kept-owned",
    breaks: "a field moved out of a value is dropped with it too",
    file: "src/lower/drops.rs",
    find: "            self.drop_state.part_flags.get(&(*var, path.clone())).cloned()\n",
    replace: "            None\n",
    tests: ["test/corpus.test.ts", "-t", "drop_partial"],
  },
  {
    name: "pattern-parts-kept-owned",
    breaks: "a part a `match` arm or a `let` pattern moves out is dropped with what it's matched against too",
    file: "src/lower/drops.rs",
    find: "        for path in self.pattern_paths(pat).unwrap_or_default() {\n",
    replace: "        for path in Vec::<Path>::new() {\n",
    tests: ["test/corpus.test.ts", "-t", "drop_partial"],
  },
  {
    name: "should-panic-takes-any-throw",
    breaks: "a `#[should_panic]` test passes when the JS throws a `TypeError`, not only when it panics",
    file: "src/output.rs",
    find: "    if (!(e instanceof Error && e.constructor === Error)) {\n",
    replace: "    if (false) {\n",
    tests: ["test/browser.test.ts", "-t", "fails the way Rust's would"],
  },
  {
    name: "swap-one-way",
    breaks: "`mem::swap` writes the first place and leaves the second as it was",
    file: "src/lower/calls.rs",
    find: "                out.push(StmtKind::Assign(b, old).at(js_span));\n",
    replace: "                drop((b, old));\n",
    tests: ["test/corpus.test.ts", "-t", "swap_replace"],
  },
  {
    name: "unfollowed-drops-taken",
    breaks: "a std call takes an iterator whose destructors rust-js can't follow, and drops what it skips silently",
    file: "src/lower/calls.rs",
    find: "        let holds_drops = |ty: Ty<'tcx>| self.drops(ty) != Drops::Nothing;\n",
    replace: "        let holds_drops = |ty: Ty<'tcx>| self.drops(ty) == Drops::Runs;\n",
    tests: ["test/corpus.test.ts", "-t", "drop_skipped_items"],
  },
  {
    name: "range-search-unborrowed",
    breaks: "a search on a range, `(0..n).all(f)`, is called on the range's `{ start, end }`, not its items",
    file: "src/lower/stdlib.rs",
    find: "            } if (self.is_lang_adt(self.reveal(self.thir[arg].ty), LangItem::Range)\n",
    replace: "            } if false && (self.is_lang_adt(self.reveal(self.thir[arg].ty), LangItem::Range)\n",
    tests: ["test/corpus.test.ts", "-t", "range_searches"],
  },
  {
    name: "library-type-shared",
    breaks: "a library's type is read without a copy by a crate that never changes it itself, so a clone, or `ORIGIN`, is shared",
    file: "src/lower/representation.rs",
    find: "                self.krate.foreign.in_library(adt.did())\n                    || (self.krate.library",
    replace: "                false\n                    || (self.krate.library",
    tests: ["test/crates.test.ts", "test/cargo-workspace.test.ts", "-t", "prints what native"],
  },
  {
    name: "library-vec-shared",
    breaks: "a crate using a library clones a `Vec` without a copy, which the library changes through its own method",
    file: "src/lower/representation.rs",
    find: "        (self.krate.library || self.krate.foreign.any())\n",
    replace: "        (self.krate.library && self.krate.foreign.any())\n",
    tests: ["test/crates.test.ts", "test/cargo-workspace.test.ts", "-t", "prints what native"],
  },
  {
    name: "library-generic-drops-nothing",
    breaks: "a library's generic function isn't given a `dropT`, so a consumer's value with a destructor is never dropped",
    file: "src/lower/analysis.rs",
    find: "    if library {\n        for &id in fns.keys() {\n",
    replace: "    if false {\n        for &id in fns.keys() {\n",
    tests: ["test/crates.test.ts", "test/cargo-workspace.test.ts", "-t", "prints what native"],
  },
  {
    name: "library-codec-unused",
    breaks: "a library lowers only the codecs it uses itself, so its consumers can't read its types",
    file: "src/lower/pipeline.rs",
    find: "                    (used.contains(&id) || (export_library && super::library::reachable(tcx, id))) && queued.insert(id)\n",
    replace: "                    used.contains(&id) && queued.insert(id)\n",
    tests: ["test/crates.test.ts", "test/cargo-workspace.test.ts", "-t", "prints what native"],
  },
  {
    name: "library-derive-pruned",
    breaks: "a library leaves out a derived impl only its consumers reach, such as `Debug` of a type it never prints",
    file: "src/lower/pipeline.rs",
    find: "                .filter(|&id| export_library && super::library::reachable(tcx, id)),\n",
    replace: "                .filter(|_| false),\n",
    tests: ["test/crates.test.ts", "test/cargo-workspace.test.ts", "-t", "prints what native"],
  },
  {
    name: "library-crate-hash-unchecked",
    breaks: "a library's manifest is taken beside the metadata of another build of it",
    file: "src/lower/library.rs",
    find: "            if let Some(library) = self.dependencies.libraries.get(name.as_str())\n",
    replace: "            if let Some(library) = self.dependencies.libraries.get(name.as_str()).filter(|_| false)\n",
    tests: ["test/crates.test.ts", "-t", "another build"],
  },
  {
    name: "library-drop-skipped",
    breaks: "a library's type's destructor isn't run by a crate that drops a value of it",
    file: "src/lower/drops.rs",
    find: "        drop.is_local() || self.krate.foreign.item(drop).is_some()\n",
    replace: "        drop.is_local()\n",
    tests: ["test/crates.test.ts", "-t", "two crates: drop"],
  },
  {
    name: "library-tuple-shared",
    breaks: "a tuple a library hands out is read without a copy, so changing it changes the library's constant",
    file: "src/lower/representation.rs",
    find: "            ty::Tuple(_) | ty::Array(..) => self.krate.library || self.krate.foreign.any(),\n",
    replace: "            ty::Tuple(_) | ty::Array(..) => false,\n",
    tests: ["test/crates.test.ts", "-t", "two crates: tuple"],
  },
  {
    name: "library-trait-no-dictionary",
    breaks: "a library's trait isn't one whose impls are dictionaries, so a call of its generic function passes none",
    file: "src/lower/recognition.rs",
    find: "        || foreign.in_library(id)\n",
    replace: "        || false\n",
    tests: ["test/crates.test.ts", "-t", "two crates: dict"],
  },
  {
    name: "library-fn-value-bare",
    breaks: "a library's generic function as a value isn't given its dictionaries",
    file: "src/lower.rs",
    find: "                    && self.is_rust_fn(def_id) =>\n",
    replace: "                    && self.krate.fns.contains_key(&def_id) =>\n",
    tests: ["test/crates.test.ts", "-t", "two crates: fnvalue"],
  },
  {
    name: "library-copy-enum-shared",
    breaks: "a copy of a library's `Copy` enum with fields is the same object",
    file: "src/lower/representation.rs",
    find: "            && (adt.did().is_local() || self.krate.foreign.in_library(adt.did()) || self.is_std_adt(ty, sym::Result)))\n",
    replace: "            && (adt.did().is_local() || self.is_std_adt(ty, sym::Result)))\n",
    tests: ["test/crates.test.ts", "-t", "two crates: copy_enum"],
  },
  {
    name: "library-recursive-clone-inline",
    breaks: "a clone of a library's type inside itself isn't a function that calls itself, and never ends",
    file: "src/lower/std_impls.rs",
    find: "        id.is_local() || self.krate.foreign.in_library(id)\n",
    replace: "        id.is_local()\n",
    tests: ["test/crates.test.ts", "-t", "two crates: tree"],
  },
  {
    name: "library-metadata-unplanned",
    breaks: "a library's metadata is published where it's asked for, unchecked, over a source of the crate",
    file: "src/output.rs",
    find: "                planned.push(path.clone());\n",
    replace: "                drop(path.clone());\n",
    tests: ["test/crates.test.ts", "-t", "a source of the crate"],
  },
  {
    name: "library-metadata-unstaged",
    breaks: "rustc writes a library's metadata where it's asked for, before its JS is published, and whether it is or not",
    file: "src/main.rs",
    find: "                    rewritten.push(format!(\"metadata={}\", staged.display()));\n",
    replace: "                    rewritten.push(format!(\"metadata={}\", path.display()));\n                    std::fs::create_dir_all(stage.clone()).ok();\n                    std::fs::write(&staged, b\"\").ok();\n",
    tests: ["test/crates.test.ts", "-t", "neither is"],
  },
  {
    name: "library-libraries-unchecked",
    breaks: "a consumer given a library but not the libraries that one was compiled against is compiled anyway",
    file: "src/library.rs",
    find: "        if let Some((library, used)) = needed.iter().find(|(_, used)| !result.libraries.contains_key(used)) {\n",
    replace: "        if let Some((library, used)) = needed.iter().find(|_| false) {\n",
    tests: ["test/crates.test.ts", "-t", "not the libraries"],
  },
  {
    name: "impl-dictionary-cache-undropped",
    breaks: "a generic impl's dictionary is cached by its dictionaries only, not the drops it's given",
    file: "src/lower/traits.rs",
    find: "                    .chain(self.given_drops().iter().map(|name| Expr::var(name)))\n",
    replace: "",
    tests: ["test/corpus.test.ts", "-t", "drop_impl_dictionary"],
  },
  {
    name: "impl-dictionary-drops-unforwarded",
    breaks: "a method through a generic impl's dictionary is given its dictionaries, but not its drops",
    file: "src/lower/traits.rs",
    find: "                true => self.evidence_args(method, instance.args, span)?,\n",
    replace: "                true => {\n                    let mut evidence = Vec::new();\n                    for bound in bounds(self.tcx, self.krate.foreign, method) {\n                        let bound = ty::EarlyBinder::bind(bound).instantiate(self.tcx, instance.args);\n                        evidence.push(self.dictionary(bound, span)?);\n                    }\n                    evidence\n                }\n",
    tests: ["test/corpus.test.ts", "-t", "drop_impl_dictionary"],
  },
  {
    name: "rustc-outputs-passed-through",
    breaks: "rustc's other outputs, `--emit=mir` say, are written past rust-js's checks",
    file: "src/main.rs",
    find: "                None => {\n                    eprintln!(\n                        \"rust-js: rustc's `--emit={kind}` isn't something rust-js writes; only a library's --emit=metadata=<path>\"\n                    );\n                    return ExitCode::FAILURE;\n                }\n",
    replace: "                None => rewritten.push(kind.to_string()),\n",
    tests: ["test/crates.test.ts", "-t", "outputs are refused"],
  },
  {
    name: "impl-dictionary-undefined-key",
    breaks: "a generic impl's dictionary given no drop is keyed by `undefined`, which a `WeakMap` can't hold",
    file: "src/runtime.rs",
    find: "  const key = keys[keys.length - 1] ?? $traitImpl;\n",
    replace: "  const key = keys[keys.length - 1];\n",
    tests: ["test/corpus.test.ts", "-t", "drop_impl_dictionary"],
  },
  {
    name: "impl-drops-unseeded",
    breaks: "a generic impl isn't given drops, so what its methods, their helpers, or a default of its trait drop is never dropped",
    file: "src/lower/analysis.rs",
    find: "        if drops_nothing_derived(tcx, imp) {\n",
    replace: "        if true {\n",
    tests: ["test/corpus.test.ts", "-t", "drop_impl"],
  },
  {
    name: "default-self-undropped",
    breaks: "a default copied into a generic impl drops nothing of its `Self`",
    file: "src/lower/traits.rs",
    find: "            drops.insert(index as u32, name);\n",
    replace: "            let _ = (index, name);\n",
    tests: ["test/corpus.test.ts", "-t", "drop_impl_indirect"],
  },
  {
    name: "default-drops-impl-indices",
    breaks: "a default copied into an impl takes the impl's drops by their indices, so `Self` drops as the impl's `T` does",
    file: "src/lower/drops.rs",
    find: "            params: std::mem::replace(&mut self.drop_state.param_drops, drops),\n",
    replace: "            params: { drop(drops); self.drop_state.param_drops.clone() },\n",
    tests: ["test/corpus.test.ts", "-t", "drop_impl_indirect"],
  },
  {
    name: "default-body-drops-unpassed",
    breaks: "a default body gives its value to a generic helper, which isn't given a drop, and drops nothing",
    file: "src/lower/analysis.rs",
    find: "        let Some(trait_id) = tcx.trait_of_assoc(method) else {\n",
    replace: "        let Some(trait_id) = tcx.trait_of_assoc(method).filter(|_| false) else {\n",
    tests: ["test/corpus.test.ts", "-t", "drop_impl_indirect"],
  },
  {
    name: "default-unsupported-drop-eager",
    breaks: "a default that only borrows is refused, for a drop of its `Self` rust-js can't make and it never needs",
    file: "src/lower/traits.rs",
    find: "                    unsupported.insert(index as u32, (t, what));\n                    continue;\n",
    replace: "                    let _ = (t, what);\n",
    tests: ["test/corpus.test.ts", "-t", "drop_impl_indirect"],
  },
  {
    name: "import-named-over-export",
    breaks: "an import takes the name of the crate's own function, which is renamed, so JS calling it by its Rust name finds none",
    file: "src/lower/analysis.rs",
    find: "    let mut reserved: HashSet<String> = uses.globals.iter().chain(taken.values().flatten()).cloned().collect();\n",
    replace: "    let mut reserved: HashSet<String> = uses.globals.iter().chain(taken.values().flatten().filter(|_| false)).cloned().collect();\n",
    tests: ["test/crates.test.ts", "-t", "two crates: same_name"],
  },
  {
    name: "cargo-build-compiled",
    breaks: "`cargo build` is given metadata and JS for what it links, and fails later, or links rustc's",
    file: "src/cargo.rs",
    find: '    if emitted.iter().any(|kind| kind == "link") {\n',
    replace: '    if emitted.iter().any(|kind| kind == "link") && false {\n',
    tests: ["test/cargo-workspace.test.ts", "-t", "done when their JS is"],
  },
  {
    name: "cargo-probe-compiled",
    breaks: "Cargo's probe of rustc, for what it prints of the target, is compiled by rust-js",
    file: "src/cargo.rs",
    find: '        .any(|flag| flag == "-" || flag == "-vV" || flag.starts_with("--print"));\n',
    replace: '        .any(|flag| flag == "-vV");\n',
    tests: ["test/cargo-workspace.test.ts", "test/cargo-react.test.ts", "-t", "Cargo workspace"],
  },
  {
    name: "cargo-app-not-library",
    breaks: "the package Cargo was asked for is built as the app, and fresh as that when another build uses it",
    file: "src/cargo.rs",
    find: '        "--library".into(),\n    ];\n',
    replace: '    ];\n    if std::env::var_os("CARGO_PRIMARY_PACKAGE").is_none() {\n        ours.push("--library".into());\n    }\n',
    tests: ["test/cargo-workspace.test.ts", "test/cargo-react.test.ts", "-t", "Cargo workspace"],
  },
  {
    name: "cargo-transitive-untold",
    breaks: "a crate is told of the libraries it names, not of those they were compiled against",
    file: "src/cargo.rs",
    find: "        .flat_map(|recorded| recorded.lines().map(PathBuf::from).collect::<Vec<_>>())\n",
    replace: "        .flat_map(|recorded| recorded.lines().take(1).map(PathBuf::from).collect::<Vec<_>>())\n",
    tests: ["test/cargo-workspace.test.ts", "test/cargo-react.test.ts", "-t", "Cargo workspace"],
  },
  {
    name: "cargo-rust-js-untracked",
    breaks: "Cargo has a crate as done when rust-js, which made its JS, has changed",
    file: "src/cargo.rs",
    find: "    first.push(' ');\n",
    replace: "    first.push(' ');\n    let exe = PathBuf::new();\n",
    tests: ["test/cargo-workspace.test.ts", "-t", "done when their JS is"],
  },
  {
    name: "cargo-missing-manifest-ignored",
    breaks: "a library of the workspace whose manifest is gone is used as if rustc had built it",
    file: "src/cargo.rs",
    find: "    if let Some(gone) = found.iter().find(|manifest| !manifest.is_file()) {\n",
    replace: "    found.retain(|manifest| manifest.is_file());\n    if let Some(gone) = found.iter().find(|manifest| !manifest.is_file()) {\n",
    tests: ["test/cargo-workspace.test.ts", "-t", "done when their JS is"],
  },
  {
    name: "cargo-shared-output",
    breaks: "each build of a crate, of a feature set say, writes one place, so a build Cargo has as done has another's JS",
    file: "src/cargo.rs",
    find: '    let dir = out_dir.join("rust-js").join(format!("{name}{extra}"));\n',
    replace: '    let dir = out_dir.join("rust-js").join(&name);\n',
    tests: ["test/cargo-workspace.test.ts", "-t", "another feature set"],
  },
  {
    name: "cargo-record-outside-plan",
    breaks: "what Cargo is told of a crate is written on its own, and a build that fails writing it has changed the JS",
    file: "src/main.rs",
    find: "                    callbacks.output.extra = extra;\n",
    replace: "                    for (path, bytes) in &extra {\n                        let _ = std::fs::write(path, bytes);\n                    }\n",
    tests: ["test/cargo-workspace.test.ts", "-t", "can't record what it made"],
  },
  {
    name: "cargo-bindings-compiled",
    breaks: "the bindings installed inside an app's workspace are its members, and compiled by rust-js, which can't",
    file: "src/cargo.rs",
    find: "BINDINGS.contains(&name.as_str())",
    replace: "BINDINGS.contains(&name.as_str()) && false",
    tests: ["test/cargo-react.test.ts", "-t", "installed inside a Cargo workspace"],
  },
  {
    name: "binding-value-bare",
    breaks: "a binding as a value is the JS function itself, which `.map` gives each index too: `parseInt(text, i)`",
    file: "src/lower/calls.rs",
    find: "            (JsForm::Call(name), None) => Expr::call(self.js_ref(&name), values),\n            (JsForm::New(name), None) => Expr::new_(self.js_ref(&name), values),\n            (JsForm::Get(name), Some(this)) if values.is_empty() && !name.contains('#') => Expr::member(this, name),",
    replace: "            (JsForm::Call(name), None) => return Ok(self.js_ref(&name)),\n            (JsForm::New(name), None) => Expr::new_(self.js_ref(&name), values),\n            (JsForm::Get(name), Some(this)) if values.is_empty() && !name.contains('#') => Expr::member(this, name),",
    tests: ["test/jsx.test.ts", "-t", "a binding is a value"],
  },
  {
    name: "binding-component-as-value",
    breaks: "a JS module's component is lowered as a value, an arrow, which isn't a JSX tag",
    file: "src/lower/jsx.rs",
    find: "                let tag = match self.binding_component(component) {\n",
    replace: "                let tag = match self.binding_component(component).filter(|_| false) {\n",
    tests: ["test/jsx.test.ts", "-t", "a binding is a value"],
  },
  {
    name: "runtime-import-missing",
    breaks: "a module compiled against @rust-js/runtime calls its helpers, and neither defines nor imports them",
    file: "src/to_oxc.rs",
    find: "    let helpers = crate::runtime::imported_helpers(&module.runtime, &generated.code);\n",
    replace: "    let helpers = Vec::<&str>::new();\n",
    tests: ["test/runtime-package.test.ts"],
  },
  {
    name: "mut-ref-index-each-use",
    breaks: "`let r = &mut v[i]; i = 2; *r += 10` writes `v[2]`: the index is evaluated at each use, not where it's borrowed",
    file: "src/lower/places.rs",
    find: "                let index = if index.is_constant() {\n",
    replace: "                let index = if true || index.is_constant() {\n",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_local"],
  },
  {
    name: "mut-ref-rebound",
    breaks: "`let c = &mut cur.count; cur = &mut b; *c += 1` changes `b`: the place follows the variable, not the object it held",
    file: "src/lower/places.rs",
    find: "Ok(self.fixed(place, rebound, out))",
    replace: "Ok(self.fixed(place, false, out))",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_local"],
  },
  {
    name: "mut-ref-rebound-element",
    breaks: "`let x = &mut cur[0]; cur = &mut b; *x += 1` of a `Vec` changes `b[0]`: `index_mut(&mut *cur, 0)` hides that it's through `cur`",
    file: "src/lower/places.rs",
    find: "ExprKind::Field { lhs, .. } | ExprKind::Index { lhs, .. } | ExprKind::Borrow { arg: lhs, .. } => {",
    replace: "ExprKind::Field { lhs, .. } | ExprKind::Index { lhs, .. } => {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_local"],
  },
  {
    name: "mut-ref-field-rebound",
    breaks: "`let x = &mut h.list[0]; h.list = &mut b; *x += 1` changes `b`: a reference in a field is taken to stay put",
    file: "src/lower/places.rs",
    find: "                    }\n                    _ => true,\n                }\n        };\n        if followed && reassignable(e) {",
    replace: "                    }\n                    ExprKind::Field { .. } => false,\n                    _ => true,\n                }\n        };\n        if followed && reassignable(e) {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_l"],
  },
  {
    name: "mut-ref-element-rebound",
    breaks: "`let x = &mut refs[0][0]; refs[0] = &mut b; *x += 1` changes `b`: only a variable's or a field's reference is taken to change",
    file: "src/lower/places.rs",
    find: "                    }\n                    _ => true,\n                }\n        };\n        if followed && reassignable(e) {",
    replace: "                    }\n                    ExprKind::Field { .. } => true,\n                    _ => false,\n                }\n        };\n        if followed && reassignable(e) {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_l"],
  },
  {
    name: "mut-ref-loop-rebound",
    breaks: "`for x in cur.iter_mut() { cur = &mut b; *x += 1 }` changes `b`: the loop follows `cur`, not the collection it started with",
    file: "src/lower/loops.rs",
    find: "        let place = if self.through_rebound(items, true) {",
    replace: "        let place = if false && self.through_rebound(items, true) {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_loop"],
  },
  {
    name: "index-receiver-after-index",
    breaks: "`cur[{ cur = &mut b; 0 }] += 1` of a `Vec` changes `b`: `index_mut(&mut *cur, ..)` takes `*cur` before the index runs",
    file: "src/lower.rs",
    find: "if self.place(arg).is_some() && !self.through_rebound(arg, false));",
    replace: "if self.place(arg).is_some());",
    tests: ["test/corpus.test.ts", "-t", "assignment_order.rs"],
  },
  {
    name: "generic-mut-param-unboxed",
    breaks: "`x: &mut T` isn't a box: `*x = v` of a generic `T` is refused",
    file: "src/lower/bodies.rs",
    find: " || self.is_generic_boxed(inner, self.typing_env.param_env)",
    replace: "",
    tests: ["test/corpus.test.ts", "-t", "generic_mut_ref"],
  },
  {
    name: "generic-mut-object-unboxed",
    breaks: "`twice(&mut c)` of a generic `&mut T` gives the object `c`, where `twice` reads a box",
    file: "src/lower/representation.rs",
    find: "self.is_boxable(inner) || self.is_generic_boxed(inner, self.tcx.param_env(fn_id))",
    replace: "self.is_boxable(inner)",
    tests: ["test/corpus.test.ts", "-t", "generic_mut_ref"],
  },
  {
    name: "dictionary-mut-self-boxed",
    breaks: "an object impl's `&mut self` method in its dictionary is given the box generic code calls it with",
    file: "src/lower/traits.rs",
    find: "if self.param_is_box(item.def_id, i) && !self.param_is_box(method, i) {",
    replace: "if false && self.param_is_box(item.def_id, i) && !self.param_is_box(method, i) {",
    tests: ["test/corpus.test.ts", "-t", "generic_mut_ref"],
  },
  {
    name: "default-self-box-to-object",
    breaks: "a default copied into an object's impl gives its `&mut self` box to the impl's methods, which take the object",
    file: "src/lower/calls.rs",
    find: "            } else {\n                ArgForm::Unboxed(id)\n",
    replace: "            } else {\n                ArgForm::Value\n",
    tests: ["test/corpus.test.ts", "-t", "generic_mut_ref"],
  },
  {
    name: "generic-mut-closure-boxed",
    breaks: "`f: &mut F` of an `F: FnMut()` is a box, which `f()` calls, where a `&mut` to a closure is the closure",
    file: "src/lower/representation.rs",
    find: "                self.tcx.fn_trait_kind_from_def_id(id).is_some()\n                    || self.tcx.is_diagnostic_item(sym::Iterator, id)",
    replace: "                false\n                    || self.tcx.is_diagnostic_item(sym::Iterator, id)",
    tests: ["test/corpus.test.ts", "-t", "closure_mut_ref"],
  },
  {
    name: "closure-mut-refused",
    breaks: "`call(&mut tick)` of a closure is refused: a `&mut` to a closure isn't the closure",
    file: "src/lower.rs",
    find: "            } if self.is_object(self.thir[arg].ty) || self.is_callable(self.thir[arg].ty) => match self.place(arg) {",
    replace: "            } if self.is_object(self.thir[arg].ty) => match self.place(arg) {",
    tests: ["test/corpus.test.ts", "-t", "closure_mut_ref"],
  },
  {
    name: "dyn-fn-mut-boxed",
    breaks: "`f: &mut dyn FnMut()` is a box, and `f()` calls `f.value()`",
    file: "src/lower/representation.rs",
    find: "            && !self.is_callable(ty)\n",
    replace: "",
    tests: ["test/corpus.test.ts", "-t", "closure_mut_ref"],
  },
  {
    name: "mut-ref-as-generic-unboxed",
    breaks: "`go(&mut x)` of a number, given where a `T` goes, is refused: a `&mut` to a number isn't a box there",
    file: "src/lower/calls.rs",
    find: "if param_box || (generic && self.is_boxable(self.thir[place].ty)) {",
    replace: "if param_box {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_as_generic"],
  },
  {
    name: "box-reborrow-as-generic-unboxed",
    breaks: "`go(&mut *x)` of a box, given where a `T` goes, gives the number in it, which the impl can't change",
    file: "src/lower/calls.rs",
    find: "            return if param_box || generic {",
    replace: "            return if param_box {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_as_generic"],
  },
  {
    name: "kept-mut-refused",
    breaks: "a `&mut` to a number kept, `Counter(&mut drops)`, is refused: it isn't a handle",
    file: "src/lower.rs",
    find: "            } if self.makes_cell(self.thir[arg].ty) => {\n",
    replace: "            } if false && self.makes_cell(self.thir[arg].ty) => {\n",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_handle"],
  },
  {
    name: "field-cell-unread",
    breaks: "`*p.left += 10` of a `&mut` kept in a field writes the field, not what its handle points at",
    file: "src/lower/places.rs",
    find: "                        ExprKind::VarRef { .. } | ExprKind::UpvarRef { .. } | ExprKind::Call { .. }\n                    ) =>\n            {\n                let (cell, _) = self.place(arg)?;\n                Some((Expr::member(cell, \"value\"), true))",
    replace: "                        ExprKind::VarRef { .. } | ExprKind::UpvarRef { .. } | ExprKind::Call { .. }\n                    ) =>\n            {\n                let (cell, _) = self.place(arg)?;\n                Some((cell, true))",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_handle"],
  },
  {
    name: "returned-borrow-boxed",
    breaks: "`pick(&mut u, &mut w)`, whose result holds the borrow, is given boxes, copied back before its result is written",
    file: "src/lower/calls.rs",
    find: "ArgForm::Boxed(place) if self.result_borrows(def_id, i) => {",
    replace: "ArgForm::Boxed(place) if false && self.result_borrows(def_id, i) => {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_handle"],
  },
  {
    name: "let-cell-unmarked",
    breaks: "`let mut cur = &mut x; *cur += 1` rebinds `cur`, which holds a handle, instead of writing through it",
    file: "src/lower/patterns.rs",
    find: "if mode.0 == ByRef::No && self.is_cell(*ty) {",
    replace: "if false && mode.0 == ByRef::No && self.is_cell(*ty) {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_handle"],
  },
  {
    name: "index-loop-over-cells",
    breaks: "`for r in refs` of a `Vec<&mut i32>` is an index loop naming `refs[i]`, so `*r *= 7` replaces the handle",
    file: "src/lower/loops.rs",
    find: "(sequence && element == Some(item) && self.place(e).is_some())",
    replace: "(sequence && self.place(e).is_some())",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_handle"],
  },
  {
    name: "rc-dyn-drop-unchecked",
    breaks: "`Rc<dyn Send>` of a value with a destructor compiles, and the destructor never runs",
    file: "src/lower/drops.rs",
    find: ".find(|(to, _)| matches!(to.kind(), ty::Dynamic(..)))",
    replace: ".find(|_| false)",
    tests: ["test/diagnostics.test.ts", "-t", "Rc<dyn> of a value with a destructor"],
  },
  {
    name: "temporary-let-refused",
    breaks: "`let x = &mut 1;` is refused: a temporary has no home as long as its `&mut` lives",
    file: "src/lower/patterns.rs",
    find: "                None if self.is_temporary(borrowed) => {",
    replace: "                None if false && self.is_temporary(borrowed) => {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_temporary"],
  },
  {
    name: "temporary-home-unwritable",
    breaks: "`if let Some(v) = &mut Some(3) { *v += 1 }` is refused: the temporary's `let` isn't taken for one",
    file: "src/lower/places.rs",
    find: "        self.locals.temporaries.contains(name)",
    replace: "        false",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_temporary"],
  },
  {
    name: "temporary-arg-refused",
    breaks: "`bump(&mut 5)` is refused: its temporary is boxed as a place, and there's nowhere to take it back to",
    file: "src/lower/calls.rs",
    find: "ArgForm::Boxed(place) if self.is_temporary(place) => {",
    replace: "ArgForm::Boxed(place) if false && self.is_temporary(place) => {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_temporary"],
  },
  {
    name: "compare-mut-cells",
    breaks: "`p == q` of two `&mut`s to numbers compares their handles, not what they point at",
    file: "src/lower/calls.rs",
    find: "            && generic_args.types().next().is_some_and(|t| self.is_cell(t))",
    replace: "            && generic_args.types().next().is_some_and(|t| false && self.is_cell(t))",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_compare"],
  },
  {
    name: "deref-pattern-on-cell",
    breaks: "`match r { &mut 3 => .. }` of a handle compares the handle with `3`",
    file: "src/lower/patterns.rs",
    find: "PatKind::Deref { subpattern, .. } if self.is_cell(pat.ty) => {",
    replace: "PatKind::Deref { subpattern, .. } if false && self.is_cell(pat.ty) => {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_compare"],
  },
  {
    name: "alias-subject-unwrapped",
    breaks: "`if let Some(n) = p` of `let p = &mut o` takes apart `o.value`, as if `o` held a handle",
    file: "src/lower/patterns.rs",
    find: "        if self.is_cell(self.thir[e].ty) && !self.is_cell_value(e) && self.place(e).is_some() {\n",
    replace: "        if self.is_cell(self.thir[e].ty) && false && self.place(e).is_some() {\n",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_local"],
  },
  {
    name: "format-without-cells",
    breaks: "`println!(\"{y}\")` of a `&mut` to a number prints its cell, `[object Object]`, not what it points at",
    file: "src/lower/format_spec.rs",
    find: "        let (value, ty) = self.through_refs(value, ty);\n",
    replace: "        let ty = ty.peel_refs();\n",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_local"],
  },
  {
    name: "debug-without-cells",
    breaks: "`{:?}` of a `Vec<&mut i32>` shows each cell, not what it points at",
    file: "src/lower/display.rs",
    find: "    pub(super) fn debug_string(&mut self, value: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {\n        let (value, ty) = self.through_refs(value, ty);",
    replace: "    pub(super) fn debug_string(&mut self, value: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {\n        let ty = ty.peel_refs();",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_compare"],
  },
  {
    name: "eq-without-cells",
    breaks: "`o == Some(&mut 7)` of an `Option<&mut i32>` compares the cells, not what they point at",
    file: "src/lower/std_impls.rs",
    find: "        let (a, _) = self.through_refs(a, ty);\n        let (b, ty) = self.through_refs(b, ty);\n        if self.is_primitive_eq(ty) {",
    replace: "        let ty = ty.peel_refs();\n        if self.is_primitive_eq(ty) {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_compare"],
  },
  {
    name: "cmp-without-cells",
    breaks: "`refs.sort()` of a `Vec<&mut i32>` orders the cells, not what they point at",
    file: "src/lower/ordering.rs",
    find: "        let (a, _) = self.through_refs(a, ty);\n        let (b, ty) = self.through_refs(b, ty);\n",
    replace: "        let ty = ty.peel_refs();\n",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_compare"],
  },
  {
    name: "primitive-eq-on-cells",
    breaks: "`==` of what holds `&mut`s to numbers is JS's `===` of the cells",
    file: "src/lower/std_impls.rs",
    find: "    fn is_primitive_eq(&self, ty: Ty<'tcx>) -> bool {\n        // A `&mut` to a number is a cell, an object (ADR 0099).\n        if self.has_cell_layer(ty) {",
    replace: "    fn is_primitive_eq(&self, ty: Ty<'tcx>) -> bool {\n        // A `&mut` to a number is a cell, an object (ADR 0099).\n        if false && self.has_cell_layer(ty) {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_compare"],
  },
  {
    name: "contains-by-identity",
    breaks: "`refs.contains(&&mut 2)` is `includes`, which compares the cells by identity",
    file: "src/lower/combinators.rs",
    find: "        // A `&mut` to one is a cell, an object (ADR 0099): not by identity.\n        if self.has_cell_layer(ty) {",
    replace: "        // A `&mut` to one is a cell, an object (ADR 0099): not by identity.\n        if false && self.has_cell_layer(ty) {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_compare"],
  },
  {
    name: "generic-mut-not-cell",
    breaks: "a generic `&mut T` kept, `result.push(&mut list.value)`, is refused as a value of type `&mut T`",
    file: "src/lower/representation.rs",
    find: "        self.is_boxable(ty) || self.is_generic_boxed(ty, self.typing_env.param_env)\n",
    replace: "        self.is_boxable(ty)\n",
    tests: ["test/corpus.test.ts","-t","generic_mut_ref_kept"],
  },
  {
    name: "generic-result-kept-as-cell",
    breaks: "`pick(true, &mut c, &mut d).n += 5` of an object `T` writes `n` on the handle, not on `c`",
    file: "src/lower/calls.rs",
    find: "                Expr::member(value, \"value\")\n",
    replace: "                value\n",
    tests: ["test/corpus.test.ts","-t","generic_mut_ref_kept"],
  },
  {
    name: "nested-mut-object-accepted",
    breaks: "`to_vec(c)` of an object gives a `Vec` of handles, which the caller takes for the objects",
    file: "src/lower/calls.rs",
    find: "        if let Some(here) = self.nested_mut_object(def_id, generic_args) {\n",
    replace: "        if let Some(here) = None::<Ty<'tcx>> {\n",
    tests: ["test/diagnostics.test.ts","-t","generic &mut T"],
  },
  {
    name: "nested-mut-object-fields-unread",
    breaks: "a generic struct's `r: &mut T` field, built of an object by the caller, is read as a cell",
    file: "src/lower/calls.rs",
    find: "                            .map(|field| field.ty(self.tcx, args).skip_normalization()),\n",
    replace: "                            .map(|field| field.ty(self.tcx, args).skip_normalization())\n                            .filter(|_| false),\n",
    tests: ["test/diagnostics.test.ts","-t","in a struct"],
  },
  {
    name: "nested-mut-object-bounds-unread",
    breaks: "`apply(c, |x| x.n += 1)` gives the closure a box of the object, and `x.n` is the box's",
    file: "src/lower/calls.rs",
    find: "        for (clause, _) in self.tcx.predicates_of(def_id).instantiate_identity(self.tcx) {\n",
    replace: "        for (clause, _) in self.tcx.predicates_of(def_id).instantiate_identity(self.tcx).into_iter().filter(|_| false) {\n",
    tests: ["test/diagnostics.test.ts","-t","given to a closure"],
  },
  {
    name: "option-as-mut-same",
    breaks: "`list.next.as_mut()` of a handle on `list.next` is the handle, not the `Option`",
    file: "src/lower/calls.rs",
    find: "            Std::Pointee => self.through_refs(arg(), self.thir[args[0]].ty).0,\n",
    replace: "            Std::Pointee => arg(),\n",
    tests: ["test/corpus.test.ts","-t","generic_mut_ref_kept"],
  },
  {
    name: "push-str-plain-place",
    breaks: "`pick(false, &mut s.0, &mut s.1).push('!')` is refused: `push_str` found only plain places",
    file: "src/lower/calls.rs",
    find: "            let (target, value) = self.prepare_assignment_target(place, true, value, span, out)?;\n",
    replace: "            let (target, value) = (super::places::PreparedPlace::Direct(self.assignee(place)?), value);\n",
    tests: ["test/corpus.test.ts","-t","generic_mut_ref_kept"],
  },
  {
    name: "callee-unresolved",
    breaks: "`checked.push(1)` through the crate's `DerefMut` pushes onto its handle, and `k.slot()` is taken for a std call's items",
    file: "src/lower/calls.rs",
    find: "        self.impl_method(def_id, args)\n            .ok()\n            .flatten()\n            .or_else(|| self.resolve_into(def_id, args))\n",
    replace: "        None.or_else(|| self.resolve_into(def_id, args))\n",
    tests: ["test/corpus.test.ts","-t","generic_mut_ref_kept"],
  },
  {
    name: "made-cell-of-generic-self",
    breaks: "`&mut self` of a default copied into an object's impl is a handle on the local, which `set_to` writes a field of",
    file: "src/lower/representation.rs",
    find: "            Some(args) => ty::EarlyBinder::bind(self.tcx, pointee)\n                .instantiate(self.tcx, args)\n                .skip_normalization(),\n",
    replace: "            Some(_) => pointee,\n",
    tests: ["test/diagnostics.test.ts","-t","default method"],
  },
  {
    name: "dyn-mut-self-given-value",
    breaks: "`d.bump()` of a `&mut dyn Counter` gives the `&mut self` method the pair's `value`, a number with no box to write",
    file: "src/lower/traits.rs",
    find: "                ty::Ref(_, _, Mutability::Mut) => pair,\n",
    replace: "                ty::Ref(_, _, Mutability::Mut) => Expr::member(pair, \"value\"),\n",
    tests: ["test/corpus.test.ts", "-t", "dyn_mut"],
  },
  {
    name: "dyn-of-cell-copied",
    breaks: "`&mut n as &mut dyn Counter` holds the handle as its `value`, and `bump` writes the handle's field, not `n`",
    file: "src/lower/traits.rs",
    find: "        if self.is_cell(source) {\n",
    replace: "        if false && self.is_cell(source) {\n",
    tests: ["test/corpus.test.ts", "-t", "dyn_mut"],
  },
  {
    name: "dyn-upcast-copied",
    breaks: "`up(s).bump()` of a `&mut dyn Sub` writes a copy of its `value`, and `n` never changes",
    file: "src/lower/traits.rs",
    find: "            if matches!(target.kind(), ty::Ref(_, _, Mutability::Mut)) {\n",
    replace: "            if false && matches!(target.kind(), ty::Ref(_, _, Mutability::Mut)) {\n",
    tests: ["test/corpus.test.ts", "-t", "dyn_mut"],
  },
  {
    name: "dyn-not-object",
    breaks: "a `&mut dyn Counter` is taken for a cell, and its pair read as `pair.value`",
    file: "src/lower/representation.rs",
    find: "            || matches!(ty.kind(), ty::Dynamic(..)) && self.dynamic_trait(ty).is_some()\n",
    replace: "            || false && matches!(ty.kind(), ty::Dynamic(..)) && self.dynamic_trait(ty).is_some()\n",
    tests: ["test/corpus.test.ts", "-t", "dyn_mut"],
  },
  {
    name: "pair-without-impl",
    breaks: "a number's `&mut dyn` pair is printed without its `impl`, and `d.impl.bump` throws",
    file: "src/to_oxc.rs",
    find: "dictionary.into_iter().chain([get, set])",
    replace: "dictionary.into_iter().take(0).chain([get, set])",
    tests: ["test/corpus.test.ts", "-t", "dyn_mut"],
  },
  {
    name: "dyn-of-generic-trait-refused",
    breaks: "`&dyn Convert<String>` of a generic trait is refused, as if its parameters had no dictionary",
    file: "src/lower/traits.rs",
    find: "    pub(super) fn dyn_supported(&self, id: DefId) -> bool {\n        self.tcx\n            .associated_items(id)\n",
    replace: "    pub(super) fn dyn_supported(&self, id: DefId) -> bool {\n        self.tcx.generics_of(id).own_params.len() <= 1\n            && self.tcx\n            .associated_items(id)\n",
    tests: ["test/corpus.test.ts", "-t", "generic_traits"],
  },
  {
    name: "supertrait-key-instantiated",
    breaks: "`Pair<u32, String>: Label<A> + Label<B>`'s impl writes its supertraits as `LabelU32` and `LabelString`, and generic code reads `LabelA` and `LabelB`",
    file: "src/lower/traits.rs",
    find: "                true => trait_word(tcx, declared),\n",
    replace: "                true => trait_word(tcx, instantiated),\n",
    tests: ["test/corpus.test.ts", "-t", "generic_traits"],
  },
  {
    name: "supertrait-key-trait-only",
    breaks: "`trait Both: Label<u32> + Label<String>` has two supertraits keyed `Label`, which collide",
    file: "src/lower/traits.rs",
    find: "                true => trait_word(tcx, declared),\n",
    replace: "                true => tcx.item_name(declared.def_id).to_string(),\n",
    tests: ["test/corpus.test.ts", "-t", "generic_traits"],
  },
  {
    name: "supertrait-bound-regions",
    breaks: "a `for<'a> Greet<&'a str>` supertrait keeps its bound lifetime, and rustc's trait selection panics on it",
    file: "src/lower/traits.rs",
    find: "            let ty::ClauseKind::Trait(instantiated) = tcx.instantiate_bound_regions_with_erased(instantiated.kind())\n            else {\n                return None;\n            };\n            Some((declared.trait_ref, instantiated.trait_ref, span))\n",
    replace: "            let ty::ClauseKind::Trait(instantiated) = instantiated.kind().skip_binder()\n            else {\n                return None;\n            };\n            Some((declared.trait_ref, instantiated.trait_ref, span))\n",
    tests: ["test/corpus.test.ts", "-t", "generic_traits"],
  },
  {
    name: "item-evidence-missing",
    breaks: "`tag` has no dictionary for `<L as Labeled>::Label: Display`, which the trait declares",
    file: "src/lower/traits.rs",
    find: "            .or_else(|| self.item_evidence(tr))\n",
    replace: "\n",
    tests: ["test/corpus.test.ts","-t","associated_types"],
  },
  {
    name: "item-bounds-unwritten",
    breaks: "an impl's dictionary has no `LabelDisplay`, and generic code reads it",
    file: "src/lower/traits.rs",
    find: "        for (name, bound) in item_bounds(self.tcx, tr.def_id, tr.args) {\n",
    replace: "        for (name, bound) in item_bounds(self.tcx, tr.def_id, tr.args).into_iter().filter(|_| false) {\n",
    tests: ["test/corpus.test.ts","-t","associated_types"],
  },
  {
    name: "projection-not-unknown",
    breaks: "`drain`'s `Vec<S::Item>` is refused as a type, and std's `I::Item` too",
    file: "src/lower/representation.rs",
    find: "            ) => self\n                .tcx\n                .try_normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(ty))\n",
    replace: "            ) if false => self\n                .tcx\n                .try_normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(ty))\n",
    tests: ["test/corpus.test.ts","-t","associated_types"],
  },
  {
    name: "projection-drops-nothing",
    breaks: "an associated type's value is taken to have nothing to drop where a type has a destructor, and its destructor never runs",
    file: "src/lower/drops.rs",
    find: "            ) if self.is_unknown(ty) => match self.may_have_destructors() {\n",
    replace: "            ) if self.is_unknown(ty) => match false {\n",
    tests: ["test/diagnostics.test.ts","-t","associated type"],
  },
  {
    name: "evidence-unnormalized",
    breaks: "`shown(&mut Words ..)` is given the dictionary of `<Words as Source>::Item: Debug`, not `String: Debug`, and it's refused",
    file: "src/lower/traits.rs",
    find: "            .try_normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(tr))\n            .unwrap_or(tr);\n",
    replace: "            .try_normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(tr))\n            .map(|_| tr)\n            .unwrap_or(tr);\n",
    tests: ["test/corpus.test.ts","-t","associated_types"],
  },
  {
    name: "item-bounds-of-gats",
    breaks: "a generic associated type's bounds are instantiated with the trait's arguments alone, and rustc panics",
    file: "src/lower/traits.rs",
    find: "            || !tcx.generics_of(item.def_id).own_params.is_empty()\n",
    replace: "\n",
    tests: ["test/diagnostics.test.ts","-t","generic associated type"],
  },
  {
    name: "instance-args-unnormalized",
    breaks: "an impl method is resolved with an associated type rustc can't normalize here, and rustc panics",
    file: "src/lower/calls.rs",
    find: "        let Ok(args) = self\n            .tcx\n            .try_normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(args))\n        else {\n            return Ok(None);\n        };\n        ty::Instance::try_resolve(",
    replace: "        ty::Instance::try_resolve(",
    tests: ["test/diagnostics.test.ts","-t","refused not crashed"],
  },
  {
    name: "extern-of-own-export",
    breaks: "an `extern` declaration of the crate's own `#[no_mangle]` function is a JS global no JS has",
    file: "src/lower/analysis.rs",
    find: "                if tcx.is_foreign_item(def_id)\n",
    replace: "                if false && tcx.is_foreign_item(def_id)\n",
    tests: ["test/diagnostics.test.ts","-t","no_mangle"],
  },
  {
    name: "own-evidence-unpassed",
    breaks: "`SShape.describe(s, label)` in generic code isn't given `T`'s `Display`, and the entry reads `undefined`",
    file: "src/lower/traits.rs",
    find: "            let mut values = values;\n            values.extend(self.own_evidence(id, generic_args, span)?);\n",
    replace: "            let values = values;\n",
    tests: ["test/corpus.test.ts","-t","generic_trait_methods"],
  },
  {
    name: "entry-evidence-in-trait-order",
    breaks: "a generic method's entry passes the trait's evidence in the trait's order, where `Square`'s `tagged` declares `A: Debug + Clone`",
    file: "src/lower/traits.rs",
    find: "                    .position(|&d| self.tcx.erase_and_anonymize_regions(d) == here)\n",
    replace: "                    .position(|_| true)\n",
    tests: ["test/corpus.test.ts","-t","generic_trait_methods"],
  },
  {
    name: "default-own-unparameterized",
    breaks: "a copied generic default, `describe<T: Display>`, takes no `TDisplay`, which its body reads",
    file: "src/lower/traits.rs",
    find: "        params.extend(own_params.into_iter().map(Into::into));\n",
    replace: "\n",
    tests: ["test/corpus.test.ts","-t","generic_trait_methods"],
  },
  {
    name: "generic-methods-with-destructors",
    breaks: "a generic trait method is accepted where a type has a destructor, which a call through a dictionary would never drop",
    file: "src/lower/traits.rs",
    find: "            && may_have_destructors(tcx, foreign)\n",
    replace: "            && false\n",
    tests: ["test/diagnostics.test.ts","-t","generic trait method"],
  },
  {
    name: "trait-const-shared",
    breaks: "`T::ZERO` of a type changed in place is one value for every use, so a `bump` of one changes the next",
    file: "src/lower/traits.rs",
    find: "                props.push(match self.contains_mutated(ty) {\n",
    replace: "                props.push(match false {\n",
    tests: ["test/corpus.test.ts","-t","associated_consts"],
  },
  {
    name: "trait-consts-all-evaluated",
    breaks: "every trait constant is in each dictionary, so `Wrap`'s `PER_SIDE`, which nothing reads, divides by zero as rustc evaluates it",
    file: "src/lower/traits.rs",
    find: "                if !self.krate.library && !self.krate.generic_consts.contains(&item.def_id) {\n",
    replace: "                if false {\n",
    tests: ["test/corpus.test.ts","-t","associated_consts"],
  },
  {
    name: "trait-const-generic-unread",
    breaks: "`S::SIDES` in generic code isn't read from `S`'s dictionary, so it's refused",
    file: "src/lower.rs",
    find: "            Some(trait_id) if args.has_non_region_param() => {\n",
    replace: "            Some(trait_id) if false => {\n",
    tests: ["test/corpus.test.ts","-t","associated_consts"],
  },
  {
    name: "generic-consts-accepted",
    breaks: "a constant with parameters of its own, `const SIZE<T>`, is accepted, which a dictionary has no place for",
    file: "src/lower/analysis.rs",
    find: "            DefKind::AssocConst { .. } if tcx.generics_of(def_id).own_params.is_empty() => continue,\n",
    replace: "            DefKind::AssocConst { .. } if true => continue,\n",
    tests: ["test/diagnostics.test.ts","-t","a generic constant"],
  },
  {
    name: "type-consts-accepted",
    breaks: "a `type const` is accepted, and rustc crashes type-checking the body it hasn't",
    file: "src/lower/analysis.rs",
    find: "            DefKind::AssocConst { is_type_const: true } => \"type constants\",\n",
    replace: "",
    tests: ["test/diagnostics.test.ts","-t","a type constant"],
  },
  {
    name: "const-args-unpassed",
    breaks: "`sum([1, 2, 3])` isn't given its `N`, and the loop runs to `undefined`",
    file: "src/lower/traits.rs",
    find: "            .map(|param| self.const_arg(args.const_at(param.index as usize), span))\n",
    replace: "            .filter(|_| false)\n            .map(|param| self.const_arg(args.const_at(param.index as usize), span))\n",
    tests: ["test/corpus.test.ts","-t","const_generics"],
  },
  {
    name: "const-param-unread",
    breaks: "`N` as a value, `count::<N>()`'s `N * 2`, is refused",
    file: "src/lower.rs",
    find: "            ExprKind::ConstParam { param, .. } => self.const_arg(ty::Const::new_param(self.tcx, param), span),\n",
    replace: "",
    tests: ["test/corpus.test.ts","-t","const_generics"],
  },
  {
    name: "repeat-generic-length",
    breaks: "`[x; N]` of a caller's `N` is refused",
    file: "src/lower.rs",
    find: "                    None => self.const_arg(count, span)?,\n",
    replace: "                    None => return Err(self.unsupported(span, \"`[x; N]` of a generic length\")),\n",
    tests: ["test/corpus.test.ts","-t","const_generics"],
  },
  {
    name: "const-dictionary-weak-cache",
    breaks: "`ringShow(2)` caches its dictionary in a `WeakMap`, which throws on the number `2`",
    file: "src/lower/traits.rs",
    find: "            let map = if self.const_params.is_empty() { \"WeakMap\" } else { \"Map\" };\n",
    replace: "            let map = \"WeakMap\";\n",
    tests: ["test/corpus.test.ts","-t","const_generics"],
  },
  {
    name: "trait-const-params-accepted",
    breaks: "a trait's const parameter, `trait Sized2<const N: usize>`, is accepted, which a copied default can't tell from its impl's",
    file: "src/lower/traits.rs",
    find: "            && (kind == DefKind::Trait\n",
    replace: "            && (false\n",
    tests: ["test/diagnostics.test.ts","-t","a trait's const parameter"],
  },
  {
    name: "generic-operators-refused",
    breaks: "`a + b` of a `T: Add` has no dictionary, and is refused",
    file: "src/lower/recognition.rs",
    find: "        || value_operator(tcx, id).is_some()\n",
    replace: "",
    tests: ["test/corpus.test.ts","-t","generic_operators"],
  },
  {
    name: "operator-entry-named-output",
    breaks: "a number's `Add` dictionary's entry is named for its `Output`, not `add`",
    file: "src/lower/traits.rs",
    find: "                .find(|item| item.is_fn())\n",
    replace: "                .next()\n",
    tests: ["test/corpus.test.ts","-t","generic_operators"],
  },
  {
    name: "operator-right-side-unchecked",
    breaks: "`f64: Mul<V2>` is a number's `*`, where it's the crate's impl, and gives `NaN`",
    file: "src/lower/traits.rs",
    find: "            && tr.args.types().all(primitive)\n",
    replace: "            && primitive(ty)\n",
    tests: ["test/corpus.test.ts","-t","generic_operators"],
  },
  {
    name: "generic-into-refused",
    breaks: "`name.into()` of an `impl Into<String>` has no dictionary, and is refused",
    file: "src/lower/recognition.rs",
    find: "        || tcx.is_diagnostic_item(sym::Into, id)\n",
    replace: "",
    tests: ["test/corpus.test.ts","-t","generic_into"],
  },
  {
    name: "known-into-through-dictionary",
    breaks: "`\"paren\".into()`, of known types, is `{ into: (s) => s }.into(\"paren\")`, not `\"paren\"`",
    file: "src/lower/traits.rs",
    find: "            && !tr.args.has_non_region_param()\n        {\n            return Ok(None);\n",
    replace: "            && false\n        {\n            return Ok(None);\n",
    tests: ["test/snapshots.test.ts","-t","calc"],
  },
  {
    name: "into-without-from",
    breaks: "`Into<Name>` of a `&str` isn't the crate's `From<&str> for Name`, and is refused",
    file: "src/lower/traits.rs",
    find: "            } else if let Some(instance) = self.resolve_instance(from, from_args)?\n                && self.is_rust_fn(instance.def_id())\n",
    replace: "            } else if let Some(instance) = self.resolve_instance(from, from_args)?\n                && false\n",
    tests: ["test/corpus.test.ts","-t","generic_into"],
  },
  {
    name: "into-itself-refused",
    breaks: "`Into<Name>` of a `Name`, which is the value, is refused",
    file: "src/lower/traits.rs",
    find: "            } else if self.tcx.erase_and_anonymize_regions(ty) == self.tcx.erase_and_anonymize_regions(target) {\n",
    replace: "            } else if false {\n",
    tests: ["test/corpus.test.ts","-t","generic_into"],
  },
  {
    name: "layout-in-function-environment",
    breaks: "`size_of_val` of a generic `async fn`'s future is laid out in the function's environment, where 1.98 finds it too generic, and is refused",
    file: "src/lower/calls.rs",
    find: "                    .layout_of(ty::TypingEnv::fully_monomorphized().as_query_input(of))\n",
    replace: "                    .layout_of(self.typing_env.as_query_input(of))\n",
    tests: ["test/corpus.test.ts","-t","future_sizes"],
  },
  {
    name: "externally-implementable-accepted",
    breaks: "`#[eii] static HELLO: u64;` is accepted, and its JS reads a `HELLO` nothing defines",
    file: "src/lower/analysis.rs",
    find: "            _ if find_attr!(tcx, def_id, EiiImpls(..) | EiiDeclaration(..) | RustcEiiForeignItem) => {\n",
    replace: "            _ if false => {\n",
    tests: ["test/diagnostics.test.ts","-t","externally implementable"],
  },
  {
    name: "rust-js-tool-unregistered",
    breaks: "`#[rust_js::link_name]` names a tool rustc doesn't know, and a binding is an error",
    file: "src/main.rs",
    find: "            tools.insert(Ident::with_dummy_span(Symbol::intern(\"rust_js\")));\n",
    replace: "",
    tests: ["test/diagnostics.test.ts","-t","unstable feature rust-js"],
  },
  {
    name: "jsx-spans-unmarked",
    breaks: "JSX's `#[rust_js::jsx] f(..)` is an expression's attribute a stable release refuses",
    file: "src/jsx_syntax.rs",
    find: "    let span = expanded(sess, span);\n",
    replace: "",
    tests: ["test/jsx.test.ts","-t","JSX supports components across modules"],
  },
  {
    name: "props-macro-not-allowed",
    breaks: "a component's props `macro` writes `#[rust_js::jsx]` on an expression, which its expansion may not",
    file: "src/jsx_syntax/parser.rs",
    find: "        format!(\"#[allow_internal_unstable(stmt_expr_attributes)] macro {ident} {body}\"),\n",
    replace: "        format!(\"macro {ident} {body}\"),\n",
    tests: ["test/crates.test.ts","-t","component of another crate"],
  },
  {
    name: "js-import-unread",
    breaks: "`js::import!(\"./app.css\");` imports nothing",
    file: "src/lower/pipeline.rs",
    find: "            for attr in super::bindings::marks(tcx, module, \"import\") {\n",
    replace: "            for attr in super::bindings::marks(tcx, module, \"none\") {\n",
    tests: ["test/compiler.test.ts","-t","js::import"],
  },
  {
    name: "js-import-const-written",
    breaks: "`js::import!`'s `const _` is written to the JS as a constant",
    file: "src/lower/analysis.rs",
    find: " && !bindings::is_mark(tcx, d.to_def_id())\n",
    replace: "\n",
    tests: ["test/compiler.test.ts","-t","js::import"],
  },
  {
    name: "camel-case-unread",
    breaks: "`js::camel_case!();` names nothing the JS way",
    file: "src/lower/bindings.rs",
    find: "    marks(tcx, LocalModDefId::CRATE_DEF_ID, \"camel_case\").next().is_some()\n",
    replace: "    marks(tcx, LocalModDefId::CRATE_DEF_ID, \"none\").next().is_some()\n",
    tests: ["test/react.test.ts","-t","camel_case crate"],
  },
  {
    name: "crate-id-miscomputed",
    breaks: "JSX's expansions are hashed with another crate's id than rustc's",
    file: "src/main.rs",
    find: "    StableCrateId::new(\n        name,\n",
    replace: "    StableCrateId::new(\n        Symbol::intern(\"elsewhere\"),\n",
    tests: ["test/jsx.test.ts","-t","JSX supports components across modules"],
  },
  {
    name: "js-type-of-struct-unrecognized",
    breaks: "`pub struct Element(PhantomData<JsObject>)` is a Rust struct to copy, not a JS object, now `JsObject` is a struct",
    file: "src/lower/recognition.rs",
    find: "            && marked.types().next().is_some_and(|t| matches!(t.kind(), ty::Foreign(_)) || self.is_js_object_itself(t)))\n",
    replace: "            && marked.types().next().is_some_and(|t| matches!(t.kind(), ty::Foreign(_))))\n",
    tests: ["test/snapshots.test.ts","-t","counter"],
  },
  {
    name: "rustc-mode-without-tool",
    breaks: "`rust-js --rustc` is rustc without rust-js's tool, and the binding crates don't compile",
    file: "src/main.rs",
    find: "impl Callbacks for Syntax {\n    fn config(&mut self, config: &mut rustc_interface::interface::Config) {\n        register_tool(config);\n",
    replace: "impl Callbacks for Syntax {\n    fn config(&mut self, _config: &mut rustc_interface::interface::Config) {\n",
    tests: ["test/cargo-react.test.ts"],
  },
  {
    name: "tuple-part-through-box",
    breaks: "`assert_eq!(*o, 3)` of a boxed `&mut i32` compares the box, `o`, not `o.value`",
    file: "src/lower/patterns.rs",
    find: "            let part = match self.stable_place(referent) {\n",
    replace: "            let part = match self.stable_place(self.strip_refs(referent)) {\n",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_box_parts"],
  },
  {
    name: "operator-items-refused",
    breaks: "`bump(&mut counts[1])` is refused, `index_mut`'s `&mut` taken for one a std call gives to keep",
    file: "src/lower.rs",
    find: "                if !from_hir_call {\n                    self.locals.item_calls.insert(fun);",
    replace: "                if false {\n                    self.locals.item_calls.insert(fun);",
    tests: ["test/corpus.test.ts", "-t", "trait_mut_self_value"],
  },
  {
    name: "std-items-accepted",
    breaks: "`m.get_mut(&k)` kept or passed on is taken for a cell, and `*v` reads `v.value` of the number itself",
    file: "src/lower/calls.rs",
    find: "            && !self.locals.item_calls.contains(&fun)\n",
    replace: "            && false\n",
    tests: ["test/diagnostics.test.ts", "-t", "std &mut"],
  },
  {
    name: "item-subject-refused",
    breaks: "`match map.get_mut(&key) { Some(value) => .. }` is refused, as `get_mut` kept is",
    file: "src/lower/calls.rs",
    find: "        self.locals.item_calls.insert(fun);\n        true\n",
    replace: "        false\n",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_std_items"],
  },
  {
    name: "item-bindings-as-cells",
    breaks: "`Some(v)` of `m.get_mut(&1)` is a cell, and `*v + 1` is `v.value + 1` of the number",
    file: "src/lower/patterns.rs",
    find: "                if items {\n                    self.locals.items.insert(b.var);",
    replace: "                if false {\n                    self.locals.items.insert(b.var);",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_std_items"],
  },
  {
    name: "item-binding-as-handle",
    breaks: "`k = v` of a `get_mut` item is a handle on the binding's copy, which `*k += 1` writes and the map never sees",
    file: "src/lower/places.rs",
    find: "            if self.locals.items.contains(&id) {\n",
    replace: "            if false && self.locals.items.contains(&id) {\n",
    tests: ["test/diagnostics.test.ts", "-t", "std &mut"],
  },
  {
    name: "shared-alias-reborrow-unwrapped",
    breaks: "`refs.contains(&y)`, reborrowed as `&*&y`, is given `x` itself, which `==` takes for a cell",
    file: "src/lower.rs",
    find: "                && self.thir[self.strip_refs(arg)].ty == self.thir[arg].ty\n",
    replace: "                && self.strip_refs(arg) == self.strip(arg)\n",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_compare"],
  },
  {
    name: "shared-alias-unwrapped",
    breaks: "`&y` of `let y = &mut x` is `x` itself, which what shows it takes for a cell",
    file: "src/lower.rs",
    find: "            } if self.is_cell(self.thir[arg].ty)\n",
    replace: "            } if false && self.is_cell(self.thir[arg].ty)\n",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_compare"],
  },
  {
    name: "trait-mut-self-unboxed",
    breaks: "`n.bump()` of a trait's `&mut self` method on a number is refused: its argument isn't boxed as a function's is",
    file: "src/lower/calls.rs",
    find: "                Some((method, method_args)) => (method, method_args, None),\n",
    replace: "                Some(_) => return Ok(None),\n",
    tests: ["test/corpus.test.ts", "-t", "trait_mut_self_value"],
  },
  {
    name: "mut-ref-loop-unchecked",
    breaks: "`for x in &mut v[3..9]` of four items runs to the end instead of panicking",
    file: "src/lower/loops.rs",
    find: "let end = self.spill(\"end\", Expr::call(Expr::var(\"$sliceEnd\"), args), out);",
    replace: "let end = if args.len() == 3 { args.pop().unwrap() } else { length };",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_loop_bounds"],
  },
  {
    name: "mut-ref-as-value",
    breaks: "`go(y)` of a `&mut` in a variable, a generic `T`, passes its place's value, which `*self += 1` can't write",
    file: "src/lower/places.rs",
    find: "            && !self.locals.boxes.contains(&id)\n            && matches!(ty.kind(), ty::Ref(_, _, Mutability::Mut))",
    replace: "            && !self.locals.boxes.contains(&id)\n            && false",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_kept"],
  },
  {
    name: "ref-mut-let-refused",
    breaks: "`if let Some(n) = p { *n += 1 }`, of a `&mut` to a `let` variable, is refused",
    file: "src/lower/patterns.rs",
    find: "&& self.is_let(&b.place)",
    replace: "&& false && self.is_let(&b.place)",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_local"],
  },
  {
    name: "ref-mut-object-let",
    breaks: "`*n = P { .. }` of a `ref mut` binding of `*cur`, a `&mut P` in a `let mut`, assigns `cur` instead of replacing `a`",
    file: "src/lower/patterns.rs",
    find: "matches!(*b.ty.kind(), ty::Ref(_, inner, _) if !self.is_object(inner))",
    replace: "matches!(*b.ty.kind(), ty::Ref(..))",
    tests: ["test/diagnostics.test.ts", "-t", "ref mut through a reference variable"],
  },
];

// Where the mutated crate is built, and the compilers kept: one copy of
// the crate, remade for each mutation, and one target, so only rust-js is
// built again.
const work = join(root, "target", "mutants");
const crate = join(work, "crate");
const crateFiles = ["Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain.toml", "src"];
const buildTimeout = 20 * 60_000;
const testTimeout = 20 * 60_000;

/** The source as it is, with `mutation` in it, or why it can't be. */
export function mutate(source: string, mutation: Mutation): string | { problem: string } {
  const at = source.indexOf(mutation.find);
  if (at < 0) return { problem: `doesn't apply: ${mutation.file} has no \`${mutation.find.split("\n")[0].trim()}\`` };
  if (source.indexOf(mutation.find, at + 1) >= 0) return { problem: `applies more than once in ${mutation.file}` };
  return source.slice(0, at) + mutation.replace + source.slice(at + mutation.find.length);
}

/** A compiler built from this checkout's crate, with `mutation` in it, or
 * without one; or why it can't be built. */
function build(mutation?: Mutation): string | { problem: string } {
  rmSync(crate, { recursive: true, force: true });
  mkdirSync(crate, { recursive: true });
  for (const file of crateFiles) cpSync(join(root, file), join(crate, file), { recursive: true });
  if (mutation) {
    const file = join(crate, mutation.file);
    const mutated = mutate(readFileSync(file, "utf8"), mutation);
    if (typeof mutated !== "string") return mutated;
    writeFileSync(file, mutated);
  }
  const target = join(work, "target");
  const p = runSync(["cargo", "build", "--quiet", "--locked", "--target-dir", target], crate, buildTimeout);
  if (p.code !== 0 || stopped(p, buildTimeout)) {
    const why = stopped(p, buildTimeout) ?? p.stderr.split("\n").find((line) => line.startsWith("error")) ?? `exited ${p.code}`;
    return { problem: `doesn't build: ${why}` };
  }
  const kept = join(work, "bin", mutation?.name ?? "unmutated");
  mkdirSync(join(work, "bin"), { recursive: true });
  cpSync(join(target, "debug", "rust-js"), kept);
  return kept;
}

const count = (output: string, what: string) => Number(new RegExp(String.raw`^ (\d+) ` + what + "$", "m").exec(output)?.[1] ?? 0);

/** What a run of a mutant's tests says of it: `caught` by a test that
 * failed, the runner ending as it does when one does; `survived`, as its
 * tests ran and passed; or `inconclusive`, as the runner, its tests or
 * their hooks ran out of time, it was stopped, or failed before any test
 * did, which says nothing of it. */
export function judge(p: Exit, output: string): "caught" | "survived" | "inconclusive" {
  if (stopped(p, testTimeout)) return "inconclusive";
  if (p.code === 0) return count(output, "pass") > 0 && count(output, "fail") === 0 ? "survived" : "inconclusive";
  const failed = [...output.matchAll(/^\(fail\) (.*)$\n?(  \^ .* timed out\b)?/gm)].filter(
    (m) => !m[1].startsWith("(unnamed)") && m[2] === undefined,
  );
  return p.code === 1 && failed.length > 0 ? "caught" : "inconclusive";
}

/** How `tests` do with `compiler`, what they printed, and how many ran. */
function test(tests: string[], compiler: string): { passed: boolean; ran: number; output: string; exit: Exit } {
  // What the JS does is what's checked: a corpus snapshot differs with
  // nearly any change to the compiler, a mutation's or not.
  const p = runSync([process.execPath, "test", ...tests], root, testTimeout, {
    RUST_JS_COMPILER: compiler,
    RUST_JS_SNAPSHOTS: "ignore",
  });
  const output = p.stdout + p.stderr;
  return { passed: p.code === 0 && !stopped(p, testTimeout), ran: count(output, "pass") + count(output, "fail"), output, exit: p };
}

async function main() {
  const named = process.argv.slice(2);
  const unknown = named.filter((name) => !mutations.some((m) => m.name === name));
  if (unknown.length > 0) throw new Error(`no mutation ${unknown.join(", ")}; there are ${mutations.map((m) => m.name).join(", ")}`);
  const chosen = named.length > 0 ? mutations.filter((m) => named.includes(m.name)) : mutations;
  // Each mutation's tests pass as the compiler is, and run at all, so
  // their failing is the mutation's doing.
  const unmutated = build();
  if (typeof unmutated !== "string") throw new Error(`the compiler as it is ${unmutated.problem}`);
  // And they use the compiler they're given: with one that compiles
  // nothing, each fails, or a mutation passing them would say nothing.
  const broken = join(work, "bin", "broken");
  writeFileSync(broken, "#!/bin/sh\necho 'error: rust-js compiles nothing here' >&2\nexit 101\n");
  chmodSync(broken, 0o755);
  for (const tests of new Set(chosen.map((m) => m.tests.join("\0")))) {
    const control = test(tests.split("\0"), unmutated);
    if (!control.passed || control.ran === 0) {
      throw new Error(`\`bun test ${tests.split("\0").join(" ")}\` doesn't pass, or runs nothing, as the compiler is:\n${control.output.slice(-2000)}`);
    }
    if (test(tests.split("\0"), broken).passed) {
      throw new Error(`\`bun test ${tests.split("\0").join(" ")}\` passes with a compiler that compiles nothing: it isn't using the one it's given`);
    }
  }
  const rows: [Mutation, string][] = [];
  for (const mutation of chosen) {
    const compiler = build(mutation);
    if (typeof compiler !== "string") {
      rows.push([mutation, compiler.problem]);
      continue;
    }
    const { ran, output, exit } = test(mutation.tests, compiler);
    // Its log, whatever it says, for what it caught or didn't.
    const log = join(work, "logs", `${mutation.name}.log`);
    mkdirSync(join(work, "logs"), { recursive: true });
    writeFileSync(log, output);
    const verdict = judge(exit, output);
    rows.push([
      mutation,
      verdict === "caught" ? "caught" : verdict === "survived" ? `SURVIVED: its ${ran} tests passed with it` : `INCONCLUSIVE: no test failed, or the runner didn't end; see ${log}`,
    ]);
  }
  for (const [mutation, result] of rows) console.log(`${mutation.name}\t${result}`);
  const summary = process.env.GITHUB_STEP_SUMMARY;
  if (summary) {
    const cell = (s: string) => s.replaceAll("|", "\\|");
    const lines = ["## Mutations", "", "| Mutation | Breaks | Result |", "|---|---|---|"];
    for (const [mutation, result] of rows) lines.push(`| ${mutation.name} | ${cell(mutation.breaks)} | ${cell(result)} |`);
    writeFileSync(summary, lines.join("\n") + "\n", { flag: "a" });
  }
  const missed = rows.filter(([, result]) => result !== "caught");
  console.log(`${rows.length - missed.length} of ${rows.length} mutations caught`);
  if (missed.length > 0) process.exitCode = 1;
}

if (import.meta.main) await main();
