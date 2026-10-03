//! What a body does with its values that have destructors, found before
//! it's lowered (ADR 0098): the variables that own one, what moves them,
//! and the temporaries that hold one. A walk of the THIR that reads the
//! function's context and writes none of it.

use std::collections::{HashMap, HashSet};

use rustc_hir::{BindingMode, ByRef};
use rustc_middle::thir::visit::{self, Visitor};
use rustc_middle::thir::{
    AdtExprBase, Expr as ThirExpr, ExprId, ExprKind, LocalVarId, Pat, PatKind, StmtKind as ThirStmt, Thir,
};
use rustc_middle::ty::adjustment::PointerCoercion;
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;

use super::super::FnCx;

use super::Path;
use super::types::{Drops, describe};

/// What a body does with its values that have destructors.
#[derive(Default)]
pub(in crate::lower) struct Facts {
    /// The variables that own one, bound by value.
    pub(super) owners: HashMap<LocalVarId, Span>,
    /// Those moved somewhere, which get a flag.
    pub(super) moved: HashSet<LocalVarId>,
    /// Each use that moves one.
    pub(super) moves: HashSet<ExprId>,
    /// Each value with a destructor that lives in a temporary: one used in
    /// place, borrowed or taken apart, and one made before an operand after
    /// it that can leave early, which a call then moves.
    pub(super) temps: HashMap<ExprId, TempKind>,
    /// What a pattern moves out of each temporary it takes apart, and
    /// whether that's only on some paths, an arm's or an `if let`'s, which
    /// flags its parts as a variable's are.
    pub(super) temp_parts: HashMap<ExprId, (Vec<Path>, bool)>,
    /// The parts of each owner that are moved somewhere, which get flags of
    /// their own, and each field that moves one, with its owner and part.
    pub(super) parts: HashMap<LocalVarId, Vec<Path>>,
    pub(super) part_moves: HashMap<ExprId, (LocalVarId, Path)>,
    /// What this body does that isn't supported yet.
    pub(super) problems: Vec<(Span, String)>,
}

impl Facts {
    pub(in crate::lower) fn has_owners(&self) -> bool {
        !self.owners.is_empty()
    }
}

/// Why a value with a destructor is a temporary.
#[derive(Clone, Copy, PartialEq)]
pub(in crate::lower) enum TempKind {
    /// It's used in place, and dropped where rustc's scope tree ends it.
    Place,
    /// It's an operand, which the call or aggregate moves once every
    /// operand is evaluated; one after it that leaves early leaves it owned.
    Operand,
}

pub(super) fn binds_any(pat: &Pat<'_>, owners: &HashMap<LocalVarId, Span>) -> bool {
    let mut found = false;
    pat.walk_always(|p| {
        if let PatKind::Binding { var, .. } = p.kind {
            found |= owners.contains_key(&var);
        }
    });
    found
}

/// What a pattern takes of a value it's matched against.
#[derive(PartialEq)]
enum Taken {
    /// Nothing that has a destructor: it borrows, copies or ignores it.
    Nothing,
    /// All of it, into one variable.
    Whole,
    /// A part, which a partial move leaves the rest of.
    Part,
}

/// Find a body's owners, and what moves them, before it's lowered.
pub(super) fn find_facts<'a, 'tcx>(cx: &FnCx<'a, 'tcx>) -> Facts {
    let thir = cx.thir;
    let mut finder = Finder {
        cx,
        thir,
        ids: thir
            .exprs
            .iter_enumerated()
            .map(|(id, e)| (std::ptr::from_ref(e) as usize, id))
            .collect(),
        stack: Vec::new(),
        lets: HashMap::new(),
        facts: Facts::default(),
    };
    // A closure called once, that moves what it holds, owns what it took,
    // and drops what it didn't move as its call ends.
    if let Some(held) = finder.consumed() {
        let span = cx.tcx.def_span(cx.body_owner);
        for (var, _) in held {
            finder.facts.owners.insert(var, span);
        }
    }
    for param in &thir.params {
        if let Some(pat) = &param.pat {
            finder.visit_pat(pat);
        }
    }
    // THIR is built from the leaves up, so the body's own expression is
    // the last, and every other is reached from it.
    if let Some(root) = thir.exprs.last_index() {
        finder.visit_expr(&thir[root]);
    }
    finder.facts
}

struct Finder<'c, 'a, 'tcx> {
    cx: &'c FnCx<'a, 'tcx>,
    thir: &'a Thir<'tcx>,
    ids: HashMap<usize, ExprId>,
    /// The expressions being walked, outermost first.
    stack: Vec<ExprId>,
    /// Each `let` statement's value, and its pattern.
    lets: HashMap<ExprId, &'a Pat<'tcx>>,
    facts: Facts,
}

impl<'c, 'a, 'tcx> Finder<'c, 'a, 'tcx> {
    fn id(&self, e: &ThirExpr<'tcx>) -> ExprId {
        self.ids[&(std::ptr::from_ref(e) as usize)]
    }

    /// What `e`, the top of the walk, is used for: the expression it's in,
    /// past what only passes it on, and which of its parts `e` is.
    fn context(&self) -> (Option<ExprId>, ExprId) {
        let mut child = *self.stack.last().expect("an expression being walked");
        for &parent in self.stack.iter().rev().skip(1) {
            // A `let`'s value is its statement's, not the block's around it:
            // its pattern says what it takes.
            if self.lets.contains_key(&child) {
                return (None, child);
            }
            match self.thir[parent].kind {
                ExprKind::Scope { .. }
                | ExprKind::Use { .. }
                | ExprKind::ValueTypeAscription { .. }
                | ExprKind::PlaceTypeAscription { .. } => child = parent,
                _ => return (Some(parent), child),
            }
        }
        (None, child)
    }

    /// Whether a value made here has a destructor to run: `None` of an
    /// `Option` that could hold one doesn't, nor does a variant of an enum
    /// without a `Drop` of its own whose fields have none.
    fn holds_drops(&self, expr: &ThirExpr<'tcx>) -> bool {
        if !self.cx.has_drops(expr.ty) {
            return false;
        }
        match &expr.kind {
            ExprKind::Adt(adt) => {
                self.cx
                    .tcx
                    .adt_destructor(adt.adt_def.did())
                    .is_some_and(|d| self.cx.runs_drop(d.did))
                    || adt.fields.iter().any(|f| self.cx.has_drops(self.thir[f.expr].ty))
                    || !matches!(adt.base, AdtExprBase::None)
            }
            _ => true,
        }
    }

    fn problem(&mut self, span: Span, what: &str) {
        self.facts.problems.push((span, what.to_string()));
    }

    /// What the body, if it's a closure's that's called once, holds that
    /// has a destructor.
    fn consumed(&self) -> Option<Vec<(LocalVarId, Ty<'tcx>)>> {
        let closure = self.cx.body_owner;
        if !self.cx.tcx.is_closure_like(closure) {
            return None;
        }
        let ty = self.cx.tcx.type_of(closure).instantiate_identity().skip_normalization();
        let ty::Closure(_, args) = ty.kind() else { return None };
        if args.as_closure().kind() != ty::ClosureKind::FnOnce {
            return None;
        }
        self.cx.held(closure)
    }

    /// A closure that holds a value with a destructor drops it through the
    /// variables it took, which JS sees only where they're seen: it's made
    /// for a `let` or a call, and written over nowhere. One its own body
    /// doesn't call once isn't given to a call by value, which may consume it.
    fn closure_checked(&mut self, expr: &ThirExpr<'tcx>) {
        match expr.kind {
            ExprKind::Closure(_) => match self.cx.drops(expr.ty) {
                Drops::Nothing => {}
                Drops::Unsupported(t, what) => self.problem(expr.span, &describe(t, what)),
                Drops::Runs => {
                    let (parent, child) = self.context();
                    let placed = match parent.map(|p| &self.thir[p].kind) {
                        None => self.lets.contains_key(&child),
                        Some(ExprKind::Call { args, .. }) => args.contains(&child),
                        _ => false,
                    };
                    if !placed {
                        self.problem(expr.span, "a closure that holds a value with a destructor, made here");
                    }
                }
            },
            ExprKind::Call { ref args, .. } if args.iter().any(|&a| self.borrowing_closure(self.thir[a].ty)) => {
                self.problem(
                    expr.span,
                    "a closure that holds a value with a destructor, given away without being called once",
                );
            }
            ExprKind::Assign { lhs, .. }
                if self.thir[lhs].ty.walk().any(|part| {
                    part.as_type()
                        .is_some_and(|t| matches!(t.kind(), ty::Closure(..)) && self.cx.drops(t) != Drops::Nothing)
                }) =>
            {
                self.problem(expr.span, "assigning a closure that holds a value with a destructor");
            }
            _ => {}
        }
    }

    /// Whether `ty` is a closure that holds a value with a destructor, which
    /// a call it's given by value may consume, though it isn't called once
    /// by its own body: what that drops, the call doesn't (ADR 0098).
    fn borrowing_closure(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.kind(), ty::Closure(_, args) if args.as_closure().kind() != ty::ClosureKind::FnOnce)
            && self.cx.has_drops(ty)
    }

    fn taken(&self, pat: &Pat<'tcx>) -> Taken {
        match &pat.kind {
            PatKind::Wild => Taken::Nothing,
            PatKind::Binding {
                mode: BindingMode(ByRef::No, _),
                subpattern: None,
                ty,
                ..
            } if self.cx.has_drops(*ty) => Taken::Whole,
            _ => {
                let mut part = false;
                pat.walk_always(|p| {
                    if let PatKind::Binding {
                        mode: BindingMode(ByRef::No, _),
                        ty,
                        ..
                    } = p.kind
                    {
                        part |= self.cx.has_drops(ty);
                    }
                });
                if part { Taken::Part } else { Taken::Nothing }
            }
        }
    }

    /// A use of `var`, an owner, at the top of the walk.
    fn owner_used(&mut self, e: ExprId, var: LocalVarId) {
        let span = self.thir[e].span;
        let (parent, child) = self.context();
        let taken = match parent.map(|p| &self.thir[p].kind) {
            None => match self.lets.get(&child) {
                // `let (c, d) = b;` moves the parts it binds.
                Some(pat) if self.taken(pat) == Taken::Part => {
                    match self.cx.pattern_paths(pat) {
                        Some(paths) => self.facts.parts.entry(var).or_default().extend(paths),
                        None => self.problem(span, "moving part of a value with a destructor"),
                    }
                    return;
                }
                Some(pat) => self.taken(pat),
                // A statement of its own, `x;`, moves it, and drops it.
                None => Taken::Whole,
            },
            Some(ExprKind::Borrow { arg, .. } | ExprKind::RawBorrow { arg, .. }) if *arg == child => Taken::Nothing,
            Some(ExprKind::Index { lhs, .. } | ExprKind::AssignOp { lhs, .. }) if *lhs == child => Taken::Nothing,
            // Written over: its old value is dropped where it's lowered.
            Some(ExprKind::Assign { lhs, .. }) if *lhs == child => Taken::Nothing,
            // What a box holds, `*b`: borrowed, as a method's `&self` takes it,
            // or moved out, which leaves the box nothing to drop.
            Some(ExprKind::Deref { arg }) if *arg == child => match self.projection_moved() {
                true => Taken::Whole,
                false => Taken::Nothing,
            },
            Some(ExprKind::Field { lhs, .. }) if *lhs == child => {
                // A field moved out, `consume(pair.a)`: a part of its own.
                match self.moved_projection() {
                    Ok(Some((field, path))) => {
                        self.facts.parts.entry(var).or_default().push(path.clone());
                        self.facts.part_moves.insert(field, (var, path));
                    }
                    Ok(None) => {}
                    Err(()) => self.problem(span, "moving part of a value with a destructor"),
                }
                return;
            }
            Some(ExprKind::Match { scrutinee, arms, .. }) if *scrutinee == child => {
                let taken: Vec<Taken> = arms.iter().map(|&a| self.taken(&self.thir[a].pattern)).collect();
                if taken.contains(&Taken::Part) {
                    // Each arm moves the parts its pattern binds.
                    let mut paths = Vec::new();
                    for (&arm, taken) in arms.iter().zip(&taken) {
                        match (taken, self.cx.pattern_paths(&self.thir[arm].pattern)) {
                            (Taken::Whole, _) | (_, None) => {
                                self.problem(span, "moving part of a value with a destructor");
                                return;
                            }
                            (_, Some(found)) => paths.extend(found),
                        }
                    }
                    self.facts.parts.entry(var).or_default().extend(paths);
                    return;
                } else if taken.contains(&Taken::Whole) {
                    Taken::Whole
                } else {
                    Taken::Nothing
                }
            }
            // `if let (Some(c), _) = b` moves the parts it binds, if it matches.
            Some(ExprKind::Let { expr, pat }) if *expr == child && self.taken(pat) == Taken::Part => {
                match self.cx.pattern_paths(pat) {
                    Some(paths) => self.facts.parts.entry(var).or_default().extend(paths),
                    None => self.problem(span, "moving part of a value with a destructor"),
                }
                return;
            }
            Some(ExprKind::Let { expr, pat }) if *expr == child => self.taken(pat),
            Some(ExprKind::Adt(adt)) if matches!(adt.base, AdtExprBase::Base(ref fru) if fru.base == child) => {
                self.problem(span, "a struct update from a value with a destructor");
                return;
            }
            _ => Taken::Whole,
        };
        match taken {
            Taken::Nothing => {}
            Taken::Whole => {
                self.facts.moved.insert(var);
                self.facts.moves.insert(e);
            }
            Taken::Part => self.problem(span, "moving part of a value with a destructor"),
        }
    }

    /// The field of a projection chain from the variable at the top of the
    /// walk that's used as a value, moving it, and its path: none if what's
    /// used has nothing to drop, and an error through what isn't a struct or
    /// a tuple, as a `Box`.
    fn moved_projection(&self) -> Result<Option<(ExprId, Path)>, ()> {
        if !self.projection_moved() {
            return Ok(None);
        }
        let mut child = *self.stack.last().expect("the variable");
        let mut field = child;
        let mut path = Path::new();
        for &parent in self.stack.iter().rev().skip(1) {
            match &self.thir[parent].kind {
                ExprKind::Scope { .. } | ExprKind::PlaceTypeAscription { .. } => {}
                ExprKind::Field { lhs, name, .. } if *lhs == child => {
                    let ty = self.thir[*lhs].ty;
                    if !matches!(ty.kind(), ty::Tuple(_)) && !matches!(ty.kind(), ty::Adt(adt, _) if adt.is_struct()) {
                        return Err(());
                    }
                    path.push((None, name.as_usize()));
                    field = parent;
                }
                _ => break,
            }
            child = parent;
        }
        Ok(self.cx.has_drops(self.thir[field].ty).then_some((field, path)))
    }

    /// Whether the field at the top of the walk, of a projection chain, is
    /// used as a value, which moves it, rather than borrowed or written.
    fn projection_moved(&self) -> bool {
        let mut child = *self.stack.last().expect("the variable");
        for &parent in self.stack.iter().rev().skip(1) {
            match &self.thir[parent].kind {
                ExprKind::Scope { .. } | ExprKind::PlaceTypeAscription { .. } => {}
                // A box's value, `*b`, as a field's: borrowed, or moved out.
                ExprKind::Field { lhs, .. } | ExprKind::Deref { arg: lhs } if *lhs == child => {}
                ExprKind::Borrow { arg, .. } | ExprKind::RawBorrow { arg, .. } if *arg == child => return false,
                ExprKind::Assign { lhs, .. } | ExprKind::AssignOp { lhs, .. } | ExprKind::Index { lhs, .. }
                    if *lhs == child =>
                {
                    return false;
                }
                _ => return self.cx.has_drops(self.thir[child].ty),
            }
            child = parent;
        }
        self.cx.has_drops(self.thir[child].ty)
    }

    /// What `pats`, the patterns a temporary `e` is matched against, move out
    /// of it: the parts each binds by value. False if one moves a part a way
    /// `pattern_paths` doesn't follow.
    fn temp_taken_apart(&mut self, e: ExprId, pats: &mut dyn Iterator<Item = &Pat<'tcx>>, flagged: bool) -> bool {
        let mut paths = Vec::new();
        for pat in pats {
            match self.cx.pattern_paths(pat) {
                Some(found) => paths.extend(found),
                None => return false,
            }
        }
        self.facts.temp_parts.insert(e, (paths, flagged));
        true
    }

    /// A value with a destructor, made here, at the top of the walk: one
    /// borrowed, or taken apart, is a temporary, dropped at the end of its
    /// statement. So is one made before an
    /// operand after it that can leave early, which drops it as it leaves.
    fn value_made(&mut self, e: ExprId) {
        let (parent, child) = self.context();
        let span = self.thir[e].span;
        let siblings: Vec<ExprId> = match parent.map(|p| &self.thir[p].kind) {
            Some(ExprKind::Call { args, .. }) => args.to_vec(),
            Some(ExprKind::Tuple { fields } | ExprKind::Array { fields }) => fields.to_vec(),
            Some(ExprKind::Adt(adt)) => adt.fields.iter().map(|f| f.expr).collect(),
            _ => Vec::new(),
        };
        if let Some(at) = siblings.iter().position(|&s| s == child)
            && siblings[at + 1..].iter().any(|&s| !self.cx.cannot_leave(s))
        {
            self.facts.temps.insert(e, TempKind::Operand);
            return;
        }
        let taken = |finder: &Self, pats: &mut dyn Iterator<Item = &Pat<'tcx>>| {
            pats.map(|p| finder.taken(p)).any(|t| t != Taken::Nothing)
        };
        let used_in_place = match parent.map(|p| &self.thir[p].kind) {
            // `let (a, _) = (x, y);` takes a temporary apart, and drops the rest.
            // `let (a, _) = (x, y);` moves what it binds out of a temporary,
            // which drops the rest as the statement ends.
            None => {
                let Some(&pat) = self.lets.get(&child) else {
                    return;
                };
                if matches!(pat.kind, PatKind::Wild | PatKind::Binding { subpattern: None, .. }) {
                    return;
                }
                if !self.temp_taken_apart(e, &mut std::iter::once(pat), false) {
                    self.problem(span, "taking apart a temporary with a destructor");
                    return;
                }
                true
            }
            Some(ExprKind::Let { expr, pat }) if *expr == child => {
                if taken(self, &mut std::iter::once(&**pat))
                    && !self.temp_taken_apart(e, &mut std::iter::once(&**pat), true)
                {
                    self.problem(span, "moving part of a temporary with a destructor");
                    return;
                }
                true
            }
            Some(ExprKind::Match { scrutinee, arms, .. }) if *scrutinee == child => {
                if taken(self, &mut arms.iter().map(|&a| &*self.thir[a].pattern))
                    && !self.temp_taken_apart(e, &mut arms.iter().map(|&a| &*self.thir[a].pattern), true)
                {
                    self.problem(span, "moving part of a temporary with a destructor");
                    return;
                }
                true
            }
            Some(
                ExprKind::Borrow { arg, .. }
                | ExprKind::RawBorrow { arg, .. }
                | ExprKind::Field { lhs: arg, .. }
                | ExprKind::Index { lhs: arg, .. }
                | ExprKind::Deref { arg },
            ) => *arg == child,
            Some(ExprKind::Adt(adt)) if matches!(adt.base, AdtExprBase::Base(ref fru) if fru.base == child) => {
                self.problem(span, "a struct update from a value with a destructor");
                return;
            }
            _ => false,
        };
        if used_in_place {
            self.facts.temps.insert(e, TempKind::Place);
        }
    }
}

/// A place: a variable, or a part of one, or what a reference points to.
pub(in crate::lower) fn is_place(kind: &ExprKind<'_>) -> bool {
    matches!(
        kind,
        ExprKind::VarRef { .. }
            | ExprKind::UpvarRef { .. }
            | ExprKind::Field { .. }
            | ExprKind::Index { .. }
            | ExprKind::Deref { .. }
            | ExprKind::StaticRef { .. }
            | ExprKind::Scope { .. }
            | ExprKind::Use { .. }
            | ExprKind::ValueTypeAscription { .. }
            | ExprKind::PlaceTypeAscription { .. }
            | ExprKind::NeverToAny { .. }
    )
}

impl<'c, 'a, 'tcx> Visitor<'a, 'tcx> for Finder<'c, 'a, 'tcx> {
    fn thir(&self) -> &'a Thir<'tcx> {
        self.thir
    }

    fn visit_stmt(&mut self, stmt: &'a rustc_middle::thir::Stmt<'tcx>) {
        if let ThirStmt::Let {
            initializer: Some(init),
            pattern,
            ..
        } = &stmt.kind
        {
            self.lets.insert(*init, pattern);
        }
        visit::walk_stmt(self, stmt);
    }

    fn visit_pat(&mut self, pat: &'a Pat<'tcx>) {
        if let PatKind::Binding {
            var,
            mode: BindingMode(ByRef::No, _),
            ty,
            ..
        } = pat.kind
        {
            match self.cx.drops(ty) {
                Drops::Nothing => {}
                Drops::Runs => {
                    self.facts.owners.insert(var, pat.span);
                }
                Drops::Unsupported(t, what) => self.problem(pat.span, &describe(t, what)),
            }
        }
        visit::walk_pat(self, pat);
    }

    fn visit_expr(&mut self, expr: &'a ThirExpr<'tcx>) {
        let id = self.id(expr);
        self.stack.push(id);
        // A `for` loop is what it iterates, its item, which its pattern owns
        // each time round as a parameter would, and its body: not the `iter`
        // and `next()` rustc writes it with, which its lowering doesn't.
        if let Some(f) = self.cx.body_query().as_for(id) {
            self.visit_expr(&self.thir[f.head]);
            self.visit_pat(f.pat);
            self.visit_expr(&self.thir[f.body]);
            self.stack.pop();
            return;
        }
        match expr.kind {
            ExprKind::VarRef { id: var } | ExprKind::UpvarRef { var_hir_id: var, .. }
                if self.facts.owners.contains_key(&var) =>
            {
                self.owner_used(id, var);
            }
            _ if !is_place(&expr.kind) && self.holds_drops(expr) => self.value_made(id),
            ExprKind::PointerCoercion {
                cast: PointerCoercion::Unsize,
                source,
                ..
            } if {
                // To a `dyn`: an array unsized to a slice is the same array.
                // What's unsized, and what to: `&D` or `Box<D>` to a `dyn`, and
                // `Rc<D>` to `Rc<dyn Send>` too, whose destructor then runs
                // through the `dyn`. Found by rustc's `issue-25515.rs`.
                let target = expr.ty;
                let from = self.thir[source].ty;
                let (to, from) = match (target.builtin_deref(true), from.builtin_deref(true)) {
                    (Some(to), Some(from)) => (to, from),
                    _ => match (target.kind(), from.kind()) {
                        (ty::Adt(_, to_args), ty::Adt(_, from_args)) => to_args
                            .types()
                            .zip(from_args.types())
                            .find(|(to, _)| matches!(to.kind(), ty::Dynamic(..)))
                            .unwrap_or((target, from)),
                        _ => (target, from),
                    },
                };
                // A reference owns nothing to drop. What owns one drops it
                // through its dictionary's `$drop`, as Rust's vtable has it:
                // a `dyn` of a trait of the crate's, or a library's, which is
                // a value and its dictionary. Of another, as `dyn Send`, or of
                // what rust-js can't drop, there's nowhere for it.
                !target.is_ref()
                    && matches!(to.kind(), ty::Dynamic(predicates, ..) if match self.cx.drops(from) {
                        Drops::Nothing => false,
                        Drops::Runs => !predicates.principal_def_id().is_some_and(|id| self.cx.is_rust_trait(id)),
                        Drops::Unsupported(..) => true,
                    })
            } =>
            {
                self.problem(expr.span, "a `dyn` of a value with a destructor");
            }
            _ => {}
        }
        self.closure_checked(expr);
        // What a closure takes, which rustc's walk doesn't reach: a capture by
        // value moves it in.
        if let ExprKind::Closure(ref closure) = expr.kind {
            for &upvar in closure.upvars.iter() {
                self.visit_expr(&self.thir[upvar]);
            }
        }
        // A generic trait method's own type parameter, given a value with a
        // destructor, would need to be given its drop, as a generic function
        // is. The trait's, and its `Self`, are the impl's, whose dictionary
        // is given their drops where it's made (ADR 0098).
        if let ty::FnDef(def_id, args) = *expr.ty.kind()
            && matches!(expr.kind, ExprKind::ZstLiteral { .. })
            && let Some(trait_id) = self.cx.tcx.trait_of_assoc(def_id)
            && self.cx.is_rust_trait(trait_id)
            && args[self.cx.tcx.generics_of(trait_id).count()..]
                .iter()
                .filter_map(|arg| arg.as_type())
                .any(|t| self.cx.drops(t) != Drops::Nothing)
        {
            self.problem(expr.span, "a generic trait method given a value with a destructor");
        }
        visit::walk_expr(self, expr);
        self.stack.pop();
    }
}
