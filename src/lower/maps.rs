//! `HashMap` and `HashSet` (ADR 0059): a JS `Map` and `Set`, whose keys are
//! what JS compares by value: numbers, strings, `char`s, `bool`s and
//! fieldless enums.

use super::stdlib::Std;
use super::{FnCx, R};
use crate::js::{self, Expr, Op, Stmt, StmtKind};
use crate::runtime::Helper;
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
    /// A `HashMap`, `HashSet`, `BTreeMap` or `BTreeSet`: a JS `Map` or `Set`.
    pub(super) fn is_map(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        ["HashMap", "HashSet", "BTreeMap", "BTreeSet"]
            .into_iter()
            .any(|name| self.is_std_adt(ty, Symbol::intern(name)))
    }

    /// A `HashSet` or `BTreeSet`: a JS `Set`.
    pub(super) fn is_set(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        self.is_std_adt(ty, Symbol::intern("HashSet")) || self.is_std_adt(ty, Symbol::intern("BTreeSet"))
    }

    /// A `BTreeMap` or `BTreeSet`, whose order is its keys' (ADR 0059).
    pub(super) fn is_sorted(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        self.is_std_adt(ty, Symbol::intern("BTreeMap")) || self.is_std_adt(ty, Symbol::intern("BTreeSet"))
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
            MapOp::New { set } => Expr::new_(Expr::var(if set { "Set" } else { "Map" }), Vec::new()),
            MapOp::From { set } => Expr::new_(Expr::var(if set { "Set" } else { "Map" }), vec![arg()]),
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
