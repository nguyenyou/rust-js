//! `HashMap` and `HashSet` (ADR 0059): a JS `Map` and `Set`, whose keys are
//! what JS compares by value: numbers, strings, `char`s, `bool`s and
//! fieldless enums.

use super::recognition::Std;
use super::representation::{Num, is_fieldless_enum};
use super::{FnCx, R};
use crate::js::{self, Expr, Op, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_hir::LangItem;
use rustc_middle::thir::{ExprId, ExprKind};
use rustc_middle::ty::{self, Ty};
use rustc_span::{Span, Symbol};

/// A `HashMap` or `HashSet` method rust-js knows.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum MapOp {
    /// `HashMap::new()`, `new Map()`, and `HashSet::new()`, `new Set()`.
    New {
        set: bool,
    },
    /// A map's `insert`: `m.set(k, v)`, or `$insert(m, k, v)` for the old value.
    Insert,
    /// A set's `insert`: `s.add(x)`, or `$add(s, x)` for whether it was new.
    Add,
    /// `get` and `get_mut`: `m.get(k)`, `undefined` for `None`.
    Get,
    /// `m[k]`: `$unwrap(m.get(k), "key not found")`, which panics as Rust's does.
    Index,
    /// `contains_key` and `contains`: `m.has(k)`.
    Has,
    /// A map's `remove`: `m.delete(k)`, or `$remove(m, k)` for the old value.
    Remove,
    /// A set's `remove`: `s.delete(x)`, whether it was there.
    Delete,
    Len,
    IsEmpty,
    /// `iter`, `keys`, `values`: an array (ADR 0036), `[...m]`.
    Iter(Part),
    /// `m.entry(k)`: `[m, k]`, for `or_insert` and the rest.
    Entry,
    OrInsert,
    OrInsertWith,
    OrDefault,
    /// `collect()` or `from` pairs: `new Map(pairs)`, or `new Set(items)`.
    From {
        set: bool,
    },
}

/// What an iterator of a map goes over.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Part {
    Entries,
    Keys,
    Values,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// A `BTreeMap` or `BTreeSet`, whose order is its keys' (ADR 0059).
    pub(super) fn is_sorted(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        self.is_std_adt(ty, Symbol::intern("BTreeMap"))
            || self.is_std_adt(ty, Symbol::intern("BTreeSet"))
            || self.is_json_map(ty)
    }

    /// What goes over a map or a set, in order: `m` itself for a hashed one,
    /// whose order is arbitrary, and `$sortedEntries(m, $cmp)` for a B-tree.
    pub(super) fn in_order_of(&mut self, map: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let ty = ty.peel_refs();
        let ty::Adt(_, args) = ty.kind() else { return Ok(map) };
        if !self.is_sorted(ty) {
            return Ok(map);
        }
        let compare = self.cmp_fn(args.type_at(0), false, span)?;
        Ok(if self.is_set(ty) {
            self.runtime.insert(Helper::SortedKeys);
            Expr::call(Expr::var("$sortedKeys"), vec![map, compare])
        } else {
            self.runtime.insert(Helper::SortedEntries);
            Expr::call(Expr::var("$sortedEntries"), vec![map, compare])
        })
    }

    /// What makes a map or a set of `key`s: `Map` and `Set`, or for a key
    /// that compares by value, `$KeyMap` and `$KeySet` (ADR 0121). One of no
    /// key type, serde_json's `Map`, is of strings.
    pub(super) fn map_class(&mut self, set: bool, key: Option<Ty<'tcx>>) -> Expr {
        let by_value = key.is_some_and(|key| self.is_value_key(key) && !self.is_js_key(key));
        let (helper, name) = match (set, by_value) {
            (false, false) => return Expr::var("Map"),
            (true, false) => return Expr::var("Set"),
            (false, true) => (Helper::KeyMap, "$KeyMap"),
            (true, true) => (Helper::KeySet, "$KeySet"),
        };
        self.runtime.insert(helper);
        Expr::var(name)
    }

    /// What a call makes: a map or a set of what its key is, as its type
    /// arguments have it, the map's among them, as `collect` and `From`
    /// have it, or else the first, `HashMap::<K, V>::new`'s; serde_json's
    /// `Map::new()` has none.
    fn made(&mut self, set: bool, generic_args: ty::GenericArgsRef<'tcx>) -> Expr {
        let map = generic_args.types().find(|&t| self.is_map(t.peel_refs()));
        let key = match map.map(|t| t.peel_refs().kind()) {
            Some(ty::Adt(_, args)) => args.types().next(),
            _ => generic_args.types().next(),
        };
        self.map_class(set, key)
    }

    /// A call of one of `op`'s kind. `discarded`: its result isn't used, so
    /// `insert` is plain `m.set(k, v)`.
    pub(super) fn map_call(
        &mut self,
        op: MapOp,
        args: &[ExprId],
        generic_args: ty::GenericArgsRef<'tcx>,
        discarded: bool,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let method = |object: Expr, name: &str, list: Vec<Expr>| Expr::call(Expr::member(object, name), list);
        let helper = |this: &mut Self, helper: Helper, name: &str, list: Vec<Expr>| {
            this.runtime.insert(helper);
            Expr::call(Expr::var(name), list)
        };
        // `or_insert` and the rest take the entry apart: `[m, k]`.
        if matches!(op, MapOp::OrInsert | MapOp::OrInsertWith | MapOp::OrDefault) {
            let (map, key) = self.entry_parts(args[0], out)?;
            let value = self.entry_value(op, args, generic_args, (map, key), span, out)?;
            return Ok(value);
        }
        let mut values = self.operands(args, out)?.into_iter();
        let mut arg = || values.next().expect("rustc checked the arguments");
        Ok(match op {
            MapOp::New { set } => Expr::new_(self.made(set, generic_args), Vec::new()),
            MapOp::From { set } => {
                let items = arg();
                Expr::new_(self.made(set, generic_args), vec![items])
            }
            MapOp::Insert if discarded => {
                let (m, k, v) = (arg(), arg(), arg());
                method(m, "set", vec![k, v])
            }
            MapOp::Insert => {
                let list = vec![arg(), arg(), arg()];
                helper(self, Helper::Insert, "$insert", list)
            }
            MapOp::Add if discarded => {
                let (s, x) = (arg(), arg());
                method(s, "add", vec![x])
            }
            MapOp::Add => {
                let list = vec![arg(), arg()];
                helper(self, Helper::Add, "$add", list)
            }
            MapOp::Get => {
                let (m, k) = (arg(), arg());
                method(m, "get", vec![k])
            }
            MapOp::Index => {
                let (m, k) = (arg(), arg());
                let value = method(m, "get", vec![k]);
                helper(self, Helper::Unwrap, "$unwrap", vec![value, Expr::str("key not found")])
            }
            MapOp::Has => {
                let (m, k) = (arg(), arg());
                method(m, "has", vec![k])
            }
            MapOp::Remove if discarded => {
                let (m, k) = (arg(), arg());
                method(m, "delete", vec![k])
            }
            MapOp::Remove => {
                let list = vec![arg(), arg()];
                helper(self, Helper::Remove, "$remove", list)
            }
            MapOp::Delete => {
                let (s, x) = (arg(), arg());
                method(s, "delete", vec![x])
            }
            MapOp::Len => Expr::member(arg(), "size"),
            MapOp::IsEmpty => Expr::bin(Op::Eq, Expr::member(arg(), "size"), Expr::int(0)),
            MapOp::Iter(part) => {
                let m = arg();
                let map_ty = self.thir[args[0]].ty;
                // A B-tree's in its keys' order.
                if self.is_sorted(map_ty) {
                    let entries = self.in_order_of(m, map_ty, span)?;
                    if self.is_set(map_ty) {
                        return Ok(entries);
                    }
                    let index = match part {
                        Part::Entries => return Ok(entries),
                        Part::Keys => 0,
                        Part::Values => 1,
                    };
                    let pick = Expr::arrow(
                        vec!["entry".into()],
                        vec![
                            StmtKind::Return(Some(Expr::index(Expr::var("entry"), Expr::int(index))))
                                .at(crate::js::Span::NONE),
                        ],
                    );
                    return Ok(method(entries, "map", vec![pick]));
                }
                let items = match part {
                    Part::Entries => m,
                    Part::Keys => method(m, "keys", Vec::new()),
                    Part::Values => method(m, "values", Vec::new()),
                };
                Expr::call(Expr::member(Expr::var("Array"), "from"), vec![items])
            }
            MapOp::Entry => Expr::array(vec![arg(), arg()]),
            MapOp::OrInsert | MapOp::OrInsertWith | MapOp::OrDefault => unreachable!("taken apart above"),
        })
    }

    /// The map and the key of `m.entry(k)`, each read more than once.
    fn entry_parts(&mut self, entry: ExprId, out: &mut Vec<Stmt>) -> R<(Expr, Expr)> {
        let (map, key) = match self.thir[self.strip(entry)].kind {
            ExprKind::Call { fun, ref args, .. } if self.std_fn(fun) == Some(Std::Map(MapOp::Entry)) => {
                (args[0], args[1])
            }
            _ => return Err(self.unsupported(self.thir[entry].span, "an entry that isn't `m.entry(k)` itself")),
        };
        let [map, key]: [Expr; 2] = self.operands(&[map, key], out)?.try_into().ok().unwrap();
        let map = if map.reads_same() {
            map
        } else {
            self.spill("map", map, out)
        };
        let key = if key.reads_same() {
            key
        } else {
            self.spill("key", key, out)
        };
        Ok((map, key))
    }

    /// `*m.entry(k).or_insert(0) += 1`, or `*m.get_mut(&k).unwrap() = v`: a
    /// value in a map, written. `None` if `e` isn't one.
    pub(super) fn map_slot(&self, e: ExprId) -> Option<ExprId> {
        let ExprKind::Deref { arg } = self.thir[self.strip(e)].kind else {
            return None;
        };
        let ExprKind::Call { fun, ref args, .. } = self.thir[self.strip(arg)].kind else {
            return None;
        };
        match self.std_fn(fun)? {
            Std::Map(MapOp::OrInsert | MapOp::OrInsertWith | MapOp::OrDefault) => Some(arg),
            // `get_mut(&k).unwrap()`.
            Std::Unwrap => match self.thir[self.strip(args[0])].kind {
                ExprKind::Call { fun, .. } if self.std_fn(fun) == Some(Std::Map(MapOp::Get)) => Some(arg),
                _ => None,
            },
            _ => None,
        }
    }

    /// Resolve a map target before writing it. Checks and entry initialization
    /// are statements, so overwriting the value cannot discard their effects.
    pub(super) fn prepare_map_place(
        &mut self,
        slot: ExprId,
        read: bool,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<MapPlace> {
        let ExprKind::Call { fun, ref args, .. } = self.thir[self.strip(slot)].kind else {
            unreachable!("checked by map_slot")
        };
        let op = self.std_fn(fun);
        let args = args.clone();
        let (map, key, checked) = match op {
            Some(Std::Map(op)) => {
                let (map, key) = self.entry_parts(args[0], out)?;
                let &ty::FnDef(_, generic_args) = self.thir[self.strip(fun)].ty.kind() else {
                    unreachable!("an entry method")
                };
                let checked = self.entry_value(op, &args, generic_args, (map.clone(), key.clone()), span, out)?;
                (map, key, checked)
            }
            _ => {
                let ExprKind::Call { args: ref get, .. } = self.thir[self.strip(args[0])].kind else {
                    unreachable!("checked by map_slot")
                };
                let [map, key]: [Expr; 2] = self.operands(&get.clone(), out)?.try_into().ok().unwrap();
                let map = if map.reads_same() {
                    map
                } else {
                    self.spill("map", map, out)
                };
                let key = if key.reads_same() {
                    key
                } else {
                    self.spill("key", key, out)
                };
                self.runtime.insert(Helper::Unwrap);
                let there = Expr::call(Expr::member(map.clone(), "get"), vec![key.clone()]);
                (map, key, Expr::call(Expr::var("$unwrap"), vec![there]))
            }
        };
        let current = if read {
            Some(self.spill("current", checked, out))
        } else {
            out.push(StmtKind::Expr(checked).at(self.js_span(span)));
            None
        };
        Ok(MapPlace { map, key, current })
    }

    /// Eager arguments stay eager; only the default factory runs lazily.
    fn entry_value(
        &mut self,
        op: MapOp,
        args: &[ExprId],
        generic_args: ty::GenericArgsRef<'tcx>,
        (map, key): (Expr, Expr),
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let default = match op {
            MapOp::OrDefault => {
                let value = generic_args
                    .types()
                    .nth(1)
                    .ok_or_else(|| self.unsupported(span, "this entry"))?;
                let value = self.default_value(value, span)?;
                Expr::arrow(Vec::new(), vec![StmtKind::Return(Some(value)).at(js::Span::NONE)])
            }
            _ => self.expr(args[1], out)?,
        };
        let (helper, name) = match op {
            MapOp::OrInsert => (Helper::OrInsert, "$orInsert"),
            _ => (Helper::OrInsertWith, "$orInsertWith"),
        };
        self.runtime.insert(helper);
        Ok(Expr::call(Expr::var(name), vec![map, key, default]))
    }

    /// Eligibility for JS Map/Set equality. A string-shaped enum alone is
    /// not enough: user equality or ordering may equate distinct variants.
    pub(super) fn is_key(&self, ty: Ty<'tcx>, ordered: bool) -> bool {
        let peeled = ty.peel_refs();
        let primitive = !peeled.is_unit()
            && !Num::of(peeled).is_some_and(Num::float)
            && self.is_primitive_key(peeled)
            && !self.has_user_impl(self.partial_eq_trait(), peeled)
            && (!ordered || !self.has_user_impl(self.ord_trait(), peeled));
        primitive || (!ordered && self.is_value_key(ty))
    }

    /// A key a `$KeyMap` finds by its value (ADR 0121): one that isn't its
    /// own JS key, and that a derived `Eq` compares field by field, as `$eq`
    /// and `$key` do.
    pub(super) fn is_value_key(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        !ty.is_unit() && !self.is_primitive_key(ty) && self.compares_by_value(ty, &mut Vec::new())
    }

    /// A key a JS `Map` finds as Rust does: a primitive one (ADR 0059), or
    /// an `Option` of one, `undefined` or the value (ADR 0030). One found by
    /// value that's one of these needs no `$KeyMap`.
    pub(super) fn is_js_key(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        self.is_primitive_key(ty)
            || self.option_of(ty).is_some_and(|inner| {
                let inner = inner.peel_refs();
                !self.boxed_payload(inner) && self.is_primitive_key(inner) && !Num::of(inner).is_some_and(Num::float)
            })
    }

    pub(super) fn is_primitive_key(&self, ty: Ty<'tcx>) -> bool {
        self.is_string_like(ty)
            || Num::of(ty).is_some()
            || ty.is_bool()
            || matches!(ty.kind(), ty::Adt(adt, _) if is_fieldless_enum(*adt))
    }

    pub(super) fn compares_by_value(&self, ty: Ty<'tcx>, seen: &mut Vec<Ty<'tcx>>) -> bool {
        let ty = ty.peel_refs();
        if seen.contains(&ty) {
            return true;
        }
        seen.push(ty);
        match ty.kind() {
            _ if Num::of(ty).is_some_and(Num::float) => false,
            _ if ty.is_unit() => true,
            _ if self.is_primitive_key(ty) => !self.has_user_impl(self.partial_eq_trait(), ty),
            ty::Tuple(parts) => parts.iter().all(|t| self.compares_by_value(t, seen)),
            ty::Array(item, _) | ty::Slice(item) => self.compares_by_value(*item, seen),
            ty::Adt(_, args) if self.is_lang_adt(ty, LangItem::Option) || ty.is_box() || self.is_vec_like(ty) => {
                self.compares_by_value(args.type_at(0), seen)
            }
            ty::Adt(adt, args) => {
                self.is_rust_adt(adt.did())
                    && self.recognition().derives(self.partial_eq_trait(), ty)
                    && adt
                        .all_fields()
                        .all(|field| self.compares_by_value(self.field_ty(field, args), seen))
            }
            _ => false,
        }
    }
}

/// An already evaluated target. Its checks have run even when no old value
/// is needed. Emission of the write never evaluates the Rust target again.
pub(super) struct MapPlace {
    map: Expr,
    key: Expr,
    current: Option<Expr>,
}

impl MapPlace {
    pub(super) fn read(&self) -> Expr {
        self.current.clone().expect("prepared for a read")
    }

    pub(super) fn write(self, value: Expr, span: js::Span, out: &mut Vec<Stmt>) {
        out.push(StmtKind::Expr(Expr::call(Expr::member(self.map, "set"), vec![self.key, value])).at(span));
    }
}
