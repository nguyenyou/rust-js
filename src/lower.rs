//! Lowering: rustc's THIR  ──►  our JS AST.
//!
//! The one idea to hold on to: Rust is *expression*-oriented (`if`, `match`,
//! `loop` and blocks all produce values), while JS separates statements from
//! expressions. So every THIR expression is lowered in one of two modes:
//!
//! - `expr()` wants a JS *expression*. Simple things (`a + f(b)`, `c ? x : y`)
//!   map directly; anything else is computed into a temporary first.
//! - `stmt()` emits JS *statements* and hands the value to a `Dest`ination:
//!   `return` it, assign it to a variable, or throw it away.
//!
//! Semantics follow Rust with `overflow-checks = off` (the release profile):
//! integer arithmetic wraps. Division by zero and `MIN / -1` still panic,
//! because Rust panics on those in every profile.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rustc_ast::{LitKind, Mutability};
use rustc_hir as hir;
use rustc_hir::def::{CtorKind, DefKind};
use rustc_hir::{BindingMode, ByRef, HirId, LangItem};
use rustc_middle::middle::region;
use rustc_middle::mir::{AssignOp, BinOp, BorrowKind, UnOp};
use rustc_middle::thir::{
    self as thir, AdtExprBase, ArmId, BlockId, BodyTy, ExprId, ExprKind, LocalVarId, LogicalOp, Pat, PatKind,
    Thir,
};
use rustc_middle::ty::adjustment::PointerCoercion;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::{DefId, LOCAL_CRATE, LocalDefId, LocalModDefId};
use rustc_span::{BytePos, ErrorGuaranteed, SourceFile, Span, Symbol, sym};

use crate::js::{self, Expr, Op, Prop, Stmt, StmtKind, UnaryOp};

type R<T> = Result<T, ErrorGuaranteed>;

/// A function's THIR, copied out of rustc before borrowck steals it.
pub struct Body<'tcx> {
    def_id: LocalDefId,
    thir: Thir<'tcx>,
    expr: ExprId,
}

/// Copy the THIR of every function and closure in the crate.
///
/// Must run *before* `analysis`: building MIR for borrowck consumes ("steals")
/// the THIR, so this is our only chance to read it.
pub fn collect_bodies(tcx: TyCtxt<'_>) -> Vec<Body<'_>> {
    let items = tcx.hir_crate_items(());
    items
        .definitions()
        .chain(items.nested_bodies())
        .filter(|&def_id| match tcx.def_kind(def_id) {
            // A function declared in an `extern` block is JS's (ADR 0021).
            DefKind::Fn => !tcx.is_foreign_item(def_id),
            DefKind::Closure => true,
            _ => false,
        })
        .filter_map(|def_id| {
            let (thir, expr) = tcx.thir_body(def_id).ok()?;
            let thir = (*thir.borrow()).clone();
            Some(Body { def_id, thir, expr })
        })
        .collect()
}

/// One Rust module's functions: a future JS file (ADR 0019).
pub struct LoweredModule {
    /// The module's path below the crate root: `[]` for the root itself,
    /// `["math", "stats"]` for `crate::math::stats`.
    pub path: Vec<String>,
    /// The `.rs` file the module's code lives in.
    pub file: Arc<SourceFile>,
    /// The modules this one calls into, as `(alias, path)`.
    pub imports: Vec<(String, Vec<String>)>,
    pub functions: Vec<js::Function>,
    /// Runtime helpers its functions use.
    pub runtime: Vec<Helper>,
}

/// Where a function ends up in the JS: its module's file, under this name.
struct FnInfo {
    module: LocalModDefId,
    name: String,
}

/// Lower every function, grouped by module. Reports all unsupported
/// features as rustc errors.
pub fn lower_crate<'tcx>(tcx: TyCtxt<'tcx>, all_bodies: &[Body<'tcx>]) -> Option<Vec<LoweredModule>> {
    let mut failed = false;
    for def_id in tcx.hir_crate_items(()).definitions() {
        let what = match tcx.def_kind(def_id) {
            // `#[derive(Clone, Copy)]` and friends write impls we never call.
            DefKind::AssocFn if tcx.is_automatically_derived(tcx.parent(def_id.to_def_id())) => continue,
            DefKind::AssocFn => "methods",
            DefKind::Const { .. } | DefKind::AssocConst { .. } => "constants",
            DefKind::Static { .. } if tcx.is_foreign_item(def_id) => continue,
            DefKind::Static { .. } => "statics",
            _ => continue,
        };
        tcx.dcx().span_err(tcx.def_span(def_id), format!("rust-js does not support {what} yet"));
        failed = true;
    }
    if failed {
        return None;
    }

    // Closures are lowered inside the function that creates them.
    let (bodies, closures): (Vec<&Body<'tcx>>, Vec<&Body<'tcx>>) =
        all_bodies.iter().partition(|body| tcx.def_kind(body.def_id) == DefKind::Fn);
    let closures: HashMap<LocalDefId, &Body<'tcx>> = closures.into_iter().map(|b| (b.def_id, b)).collect();

    // JS globals the crate uses, whether declared here or in another crate
    // (`web`, ADR 0024): every module reserves them, so a local named
    // `console` can't hide the real one.
    let mut globals: HashSet<String> = HashSet::new();
    for body in all_bodies {
        for expr in body.thir.exprs.iter() {
            let def_id = match (&expr.kind, expr.ty.kind()) {
                (ExprKind::ZstLiteral { .. }, ty::FnDef(def_id, _)) | (ExprKind::StaticRef { def_id, .. }, _) => *def_id,
                _ => continue,
            };
            if tcx.is_foreign_item(def_id) {
                globals.extend(js_global(tcx, def_id));
            }
        }
    }

    // The modules that get a JS file: the root, then every module with a
    // function, in the order their first function appears.
    let mut modules = vec![LocalModDefId::CRATE_DEF_ID];
    for body in &bodies {
        let module = tcx.parent_module_from_def_id(body.def_id);
        if !modules.contains(&module) {
            modules.push(module);
        }
    }

    // Each function's JS name, unique within its module's file. `taken` also
    // collects the import aliases below, so local variables avoid both.
    let mut taken: HashMap<LocalModDefId, HashSet<String>> =
        modules.iter().map(|&m| (m, globals.clone())).collect();
    let fns: HashMap<DefId, FnInfo> = bodies
        .iter()
        .map(|body| {
            let module = tcx.parent_module_from_def_id(body.def_id);
            let names = taken.entry(module).or_default();
            let name = fresh_in(names, tcx.item_name(body.def_id.to_def_id()).as_str());
            (body.def_id.to_def_id(), FnInfo { module, name })
        })
        .collect();

    // Which modules each module calls into, and which functions are called
    // from another module: those must be exported, even if private in Rust
    // (a child module may call its parent's private functions).
    let mut uses: HashMap<LocalModDefId, Vec<LocalModDefId>> = HashMap::new();
    let mut called_from_elsewhere: HashSet<DefId> = HashSet::new();
    for body in all_bodies {
        let from = tcx.parent_module_from_def_id(body.def_id);
        for expr in body.thir.exprs.iter() {
            if let (ExprKind::ZstLiteral { .. }, ty::FnDef(def_id, _)) = (&expr.kind, expr.ty.kind())
                && let Some(target) = fns.get(def_id)
                && target.module != from
            {
                called_from_elsewhere.insert(*def_id);
                let targets = uses.entry(from).or_default();
                if !targets.contains(&target.module) {
                    targets.push(target.module);
                }
            }
        }
    }

    // Import aliases: the module's last path segment (the crate name for the
    // root), unique within the importing file.
    let paths: HashMap<LocalModDefId, Vec<String>> =
        modules.iter().map(|&m| (m, module_path(tcx, m))).collect();
    let crate_name = tcx.crate_name(LOCAL_CRATE).to_string();
    let mut aliases: HashMap<LocalModDefId, HashMap<LocalModDefId, String>> = HashMap::new();
    for &module in &modules {
        let mut targets = uses.remove(&module).unwrap_or_default();
        targets.sort_by(|a, b| paths[a].cmp(&paths[b]));
        let names = taken.entry(module).or_default();
        let module_aliases = targets
            .into_iter()
            .map(|target| (target, fresh_in(names, paths[&target].last().unwrap_or(&crate_name))))
            .collect();
        aliases.insert(module, module_aliases);
    }

    // The types whose JS objects get changed in place somewhere in the
    // crate: `a.b.c = ..` changes the object `a.b`, so it's `a.b`'s type.
    // Only these ever need copying (ADR 0020).
    let mut mutated: HashSet<Ty<'tcx>> = HashSet::new();
    for body in all_bodies {
        for expr in body.thir.exprs.iter() {
            if let ExprKind::Assign { lhs, .. } | ExprKind::AssignOp { lhs, .. } = expr.kind
                && let ExprKind::Field { lhs: object, .. } = body.thir[strip(&body.thir, lhs)].kind
            {
                mutated.insert(body.thir[object].ty);
            }
        }
    }

    let mut functions: HashMap<LocalModDefId, Vec<js::Function>> = HashMap::new();
    let mut runtime: HashMap<LocalModDefId, HashSet<Helper>> = HashMap::new();
    for body in &bodies {
        let def_id = body.def_id.to_def_id();
        let module = fns[&def_id].module;
        let file = module_file(tcx, module);
        let mut cx = FnCx {
            tcx,
            typing_env: ty::TypingEnv::post_analysis(tcx, def_id),
            mutated: &mutated,
            closures: &closures,
            captures: HashMap::new(),
            file_start: file.start_pos,
            file_end: file.end_position(),
            thir: &body.thir,
            fns: &fns,
            module,
            aliases: &aliases[&module],
            vars: HashMap::new(),
            // Locals must never shadow a function or an import of this file.
            names: taken[&module].clone(),
            labels: HashSet::new(),
            loops: Vec::new(),
            runtime: HashSet::new(),
        };
        match cx.lower_fn(body) {
            Ok(mut lowered) => {
                lowered.function.export |= called_from_elsewhere.contains(&def_id);
                functions.entry(module).or_default().push(lowered.function);
                runtime.entry(module).or_default().extend(lowered.runtime);
            }
            Err(_) => failed = true,
        }
    }
    if failed {
        return None;
    }

    let lowered = modules
        .into_iter()
        .map(|module| {
            let mut imports: Vec<(String, Vec<String>)> = aliases[&module]
                .iter()
                .map(|(target, alias)| (alias.clone(), paths[target].clone()))
                .collect();
            imports.sort_by(|a, b| a.1.cmp(&b.1));
            let mut helpers: Vec<Helper> = runtime.remove(&module).unwrap_or_default().into_iter().collect();
            helpers.sort();
            LoweredModule {
                path: paths[&module].clone(),
                file: module_file(tcx, module),
                imports,
                functions: functions.remove(&module).unwrap_or_default(),
                runtime: helpers,
            }
        })
        .collect();
    Some(lowered)
}

/// What a JS function or global declared in an `extern` block is called:
/// its `#[link_name]`, or its Rust name. A dotted name (`console.log`) is
/// a path from a global.
fn js_name(tcx: TyCtxt<'_>, def_id: DefId) -> String {
    match tcx.codegen_fn_attrs(def_id).symbol_name {
        Some(name) => name.to_string(),
        None => tcx.item_name(def_id).to_string(),
    }
}

/// A JS function whose first parameter is named `this` is a method:
/// `f(x, a)` calls `x.f(a)`.
fn is_method(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    tcx.def_kind(def_id) == DefKind::Fn
        && matches!(tcx.fn_arg_idents(def_id).first(), Some(Some(ident)) if ident.name.as_str() == "this")
}

/// How a call to a JS function is written, from its `#[link_name]` (ADR 0024).
enum JsForm {
    /// `name(..)`, or `this.name(..)` for a method.
    Call(String),
    /// `this.name`.
    Get(String),
    /// `this.name = value`.
    Set(String),
    /// `new Name(..)`.
    New(String),
    /// `this` itself: an unchecked cast.
    This,
}

fn js_form(tcx: TyCtxt<'_>, def_id: DefId) -> JsForm {
    let name = js_name(tcx, def_id);
    if name == "this" {
        return JsForm::This;
    }
    let forms: [(&str, fn(String) -> JsForm); 3] = [("get ", JsForm::Get), ("set ", JsForm::Set), ("new ", JsForm::New)];
    for (prefix, form) in forms {
        if let Some(rest) = name.strip_prefix(prefix) {
            return form(rest.to_string());
        }
    }
    JsForm::Call(name)
}

/// The JS global a JS item refers to, if any: `document`, `console` for
/// `console.log`, `Event` for `new Event`. Methods and properties have none.
fn js_global(tcx: TyCtxt<'_>, def_id: DefId) -> Option<String> {
    let root = |name: &str| name.split('.').next().unwrap_or_default().to_string();
    match tcx.def_kind(def_id) {
        DefKind::Static { .. } => Some(root(&js_name(tcx, def_id))),
        DefKind::Fn if !is_method(tcx, def_id) => match js_form(tcx, def_id) {
            JsForm::Call(name) | JsForm::New(name) => Some(root(&name)),
            _ => None,
        },
        _ => None,
    }
}

/// A module's path below the crate root, e.g. `["math", "stats"]`.
fn module_path(tcx: TyCtxt<'_>, module: LocalModDefId) -> Vec<String> {
    if module == LocalModDefId::CRATE_DEF_ID {
        return Vec::new();
    }
    let mut path = module_path(tcx, tcx.parent_module_from_def_id(module.to_local_def_id()));
    path.push(tcx.item_name(module.to_def_id()).to_string());
    path
}

/// The `.rs` file a module's code lives in: its own file for `mod foo;`,
/// the parent's file for an inline `mod foo { .. }`.
fn module_file(tcx: TyCtxt<'_>, module: LocalModDefId) -> Arc<SourceFile> {
    let inner = tcx.hir_get_module(module).0.spans.inner_span;
    tcx.sess.source_map().lookup_source_file(inner.lo())
}

pub struct LoweredFn {
    pub function: js::Function,
    pub runtime: HashSet<Helper>,
}

/// Runtime helpers, emitted into the module only when used.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Helper {
    Div,
    Rem,
}

impl Helper {
    pub fn source(self) -> &'static str {
        match self {
            Helper::Div => {
                r#"
function $div(a, b, min) {
  if (b === 0) {
    throw new Error("attempt to divide by zero");
  }
  if (a === min && b === -1) {
    throw new Error("attempt to divide with overflow");
  }
  return a / b;
}
"#
            }
            Helper::Rem => {
                r#"
function $rem(a, b, min) {
  if (b === 0) {
    throw new Error("attempt to calculate the remainder with a divisor of zero");
  }
  if (a === min && b === -1) {
    throw new Error("attempt to calculate the remainder with overflow");
  }
  return a % b;
}
"#
            }
        }
    }
}

/// Where the value of a statement-lowered expression goes.
#[derive(Clone)]
enum Dest {
    Return,
    Assign(String),
    Discard,
}

struct Var {
    /// Usually the variable's JS name. An immutable binding into a pattern
    /// can instead just name the place it matched: `let (q, r) = t` makes
    /// `q` mean `t[0]`, with no JS variable at all.
    place: Expr,
    mutable: bool,
    /// How many loops enclose its declaration (ADR 0022).
    depth: usize,
}

/// A variable bound by a pattern, and the place in the subject it matched.
struct Binding<'tcx> {
    var: LocalVarId,
    name: String,
    mutable: bool,
    place: Expr,
    ty: Ty<'tcx>,
}

/// The std functions whose JS meaning rust-js knows (ADR 0023).
#[derive(Clone, Copy)]
enum Std {
    BoxNew,
    RcNew,
    RcClone,
    CellNew,
    CellGet,
    CellSet,
    ToString,
    /// `String + &str`.
    Concat,
    /// `Deref::deref` on a `String` or an `Rc`.
    Deref,
}

/// How a Rust struct or tuple type is represented in JS (ADR 0020).
enum Shape<'tcx> {
    /// A struct with named fields: `{ x: 1, y: 2 }`.
    Object(Vec<(String, Ty<'tcx>)>),
    /// A tuple or tuple struct: `[1, 2]`.
    Array(Vec<Ty<'tcx>>),
    /// Anything else: numbers, `bool`, unit and unit structs (`undefined`), enums.
    Other,
}

struct Loop {
    scope: region::Scope,
    /// Rust's label (`'outer`), or `loop`.
    label_base: String,
    /// Assigned on first use by a `break`/`continue` from an inner loop.
    label: Option<String>,
    /// Where `break value` delivers its value.
    dest: Dest,
}

/// Number representations. Every one of them is a plain JS number; the
/// difference is how results are wrapped back into range.
#[derive(Clone, Copy, PartialEq)]
enum Num {
    I8,
    I16,
    I32,
    U8,
    U16,
    U32,
    F64,
}

impl Num {
    fn of(ty: Ty<'_>) -> Option<Num> {
        Some(match ty.kind() {
            ty::Int(ty::IntTy::I8) => Num::I8,
            ty::Int(ty::IntTy::I16) => Num::I16,
            ty::Int(ty::IntTy::I32) => Num::I32,
            ty::Uint(ty::UintTy::U8) => Num::U8,
            ty::Uint(ty::UintTy::U16) => Num::U16,
            ty::Uint(ty::UintTy::U32) => Num::U32,
            ty::Float(ty::FloatTy::F64) => Num::F64,
            _ => return None,
        })
    }

    fn bits(self) -> u32 {
        match self {
            Num::I8 | Num::U8 => 8,
            Num::I16 | Num::U16 => 16,
            Num::I32 | Num::U32 => 32,
            Num::F64 => 64,
        }
    }

    fn signed(self) -> bool {
        matches!(self, Num::I8 | Num::I16 | Num::I32)
    }

    /// The inclusive value range, for integers.
    fn range(self) -> (i128, i128) {
        let bits = self.bits();
        if self.signed() {
            (-(1 << (bits - 1)), (1 << (bits - 1)) - 1)
        } else {
            (0, (1 << bits) - 1)
        }
    }

    /// Wrap an exact JS result back into this type's range, like Rust's
    /// wrapping arithmetic: `x | 0` for i32, `x >>> 0` for u32, and so on.
    fn wrap(self, e: Expr) -> Expr {
        match self {
            Num::I32 => Expr::bin(Op::BitOr, e, Expr::num(0)),
            Num::U32 => Expr::bin(Op::UShr, e, Expr::num(0)),
            Num::I8 | Num::I16 => {
                let shift = 32 - self.bits();
                Expr::bin(Op::Shr, Expr::bin(Op::Shl, e, Expr::num(shift)), Expr::num(shift))
            }
            Num::U8 | Num::U16 => Expr::bin(Op::BitAnd, e, Expr::int(self.range().1)),
            Num::F64 => e,
        }
    }
}

struct FnCx<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    typing_env: ty::TypingEnv<'tcx>,
    /// Types whose objects are changed in place somewhere in the crate.
    mutated: &'a HashSet<Ty<'tcx>>,
    /// Every closure's THIR, lowered where the closure is created.
    closures: &'a HashMap<LocalDefId, &'a Body<'tcx>>,
    /// While lowering a closure: the places it captured into snapshots.
    captures: HashMap<(LocalVarId, Vec<usize>), Var>,
    /// The range, in rustc's global source map, of the `.rs` file this
    /// function's module lives in, for `js_span`.
    file_start: BytePos,
    file_end: BytePos,
    thir: &'a Thir<'tcx>,
    fns: &'a HashMap<DefId, FnInfo>,
    /// The module being lowered, and its import aliases for other modules.
    module: LocalModDefId,
    aliases: &'a HashMap<LocalModDefId, String>,
    vars: HashMap<LocalVarId, Var>,
    /// JS names already taken in this function.
    names: HashSet<String>,
    labels: HashSet<String>,
    loops: Vec<Loop>,
    runtime: HashSet<Helper>,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    fn lower_fn(&mut self, body: &Body<'tcx>) -> R<LoweredFn> {
        let def_id = body.def_id.to_def_id();
        let mut out = Vec::new();
        let thir = self.thir;
        let params = self.lower_params(&thir.params.raw, self.tcx.def_span(def_id), &mut out)?;

        let BodyTy::Fn(sig) = self.thir.body_type else {
            return Err(self.unsupported(self.tcx.def_span(def_id), "this kind of body"));
        };
        let dest = if sig.output().is_unit() { Dest::Discard } else { Dest::Return };
        self.stmt(body.expr, &dest, &mut out)?;

        Ok(LoweredFn {
            function: js::Function {
                name: self.fns[&def_id].name.clone(),
                params,
                body: out,
                export: self.tcx.visibility(def_id).is_public(),
                span: self.js_span(self.tcx.def_span(def_id)),
                name_span: self.tcx.def_ident_span(def_id).map_or(js::Span::NONE, |s| self.js_span(s)),
            },
            runtime: std::mem::take(&mut self.runtime),
        })
    }

    /// Name the parameters. One with a pattern (`(x, y): (i32, i32)`) is
    /// taken whole, then taken apart at the start of the body in `out`.
    fn lower_params(&mut self, params: &[thir::Param<'tcx>], span: Span, out: &mut Vec<Stmt>) -> R<Vec<String>> {
        let mut names = Vec::new();
        for param in params {
            let span = param.ty_span.unwrap_or(span);
            self.check_value_ty(param.ty, span)?;
            let name = match param.pat.as_deref() {
                Some(pat) => match &pat.kind {
                    PatKind::Binding { name, var, mode, subpattern: None, .. } => {
                        self.check_by_value(*mode, pat.span)?;
                        self.bind(*var, name.as_str(), mode.1 == Mutability::Mut)
                    }
                    PatKind::Wild => self.fresh("_"),
                    // `(x, y): (i32, i32)`: take the whole value, then take it apart.
                    _ => {
                        let name = self.fresh("param");
                        self.destructure(pat, Expr::var(&name), true, out)?;
                        name
                    }
                },
                None => self.fresh("_"),
            };
            names.push(name);
        }
        Ok(names)
    }

    // ── Statement mode ──────────────────────────────────────────────────

    /// Emit statements that compute `e` and deliver its value to `dest`.
    fn stmt(&mut self, e: ExprId, dest: &Dest, out: &mut Vec<Stmt>) -> R<()> {
        let expr = &self.thir[e];
        // A unit value carries no information, and a never value never
        // arrives. Either way, there is nothing to deliver.
        let dest = if expr.ty.is_unit() || expr.ty.is_never() { &Dest::Discard } else { dest };
        let span = self.js_span(expr.span);

        match expr.kind {
            ExprKind::Scope { value, hir_id, region_scope } => {
                if let ExprKind::Loop { body } = self.thir[value].kind {
                    self.lower_loop(region_scope, hir_id, body, dest, span, out)
                } else {
                    self.stmt(value, dest, out)
                }
            }
            ExprKind::Use { source }
            | ExprKind::NeverToAny { source }
            | ExprKind::ValueTypeAscription { source, .. }
            | ExprKind::PlaceTypeAscription { source, .. } => self.stmt(source, dest, out),
            ExprKind::Block { block } => self.block(block, dest, out),
            ExprKind::If { cond, then, else_opt, .. } => {
                let cond = self.expr(cond, out)?;
                let mut then_out = Vec::new();
                self.stmt(then, dest, &mut then_out)?;
                let else_out = match else_opt {
                    Some(els) => {
                        let mut else_out = Vec::new();
                        self.stmt(els, dest, &mut else_out)?;
                        Some(else_out)
                    }
                    None => None,
                };
                out.push(StmtKind::If(cond, then_out, else_out).at(span));
                Ok(())
            }
            ExprKind::Match { scrutinee, ref arms, .. } => self.lower_match(scrutinee, arms, dest, out),
            ExprKind::Return { value } => {
                match value {
                    Some(v) if !self.thir[v].ty.is_unit() => self.stmt(v, &Dest::Return, out)?,
                    Some(v) => {
                        self.stmt(v, &Dest::Discard, out)?;
                        out.push(StmtKind::Return(None).at(span));
                    }
                    None => out.push(StmtKind::Return(None).at(span)),
                }
                Ok(())
            }
            ExprKind::Break { label, value } => {
                let i = self.loop_index(label, expr.span)?;
                if let Some(v) = value {
                    let loop_dest = self.loops[i].dest.clone();
                    self.stmt(v, &loop_dest, out)?;
                    if matches!(loop_dest, Dest::Return) {
                        return Ok(()); // `return` already left the loop.
                    }
                }
                let label = self.jump_label(i);
                out.push(StmtKind::Break(label).at(span));
                Ok(())
            }
            ExprKind::Continue { label } => {
                let i = self.loop_index(label, expr.span)?;
                let label = self.jump_label(i);
                out.push(StmtKind::Continue(label).at(span));
                Ok(())
            }
            // Rust evaluates the right side of an assignment first. The target
            // is a variable or its fields, which reading can't change.
            ExprKind::Assign { lhs, rhs } => {
                let target = self.assignee(lhs)?;
                match &target.kind {
                    js::ExprKind::Var(name) if !self.is_simple(rhs) => {
                        let name = name.clone();
                        self.stmt(rhs, &Dest::Assign(name), out)
                    }
                    _ => {
                        let value = self.expr(rhs, out)?;
                        out.push(StmtKind::Assign(target, value).at(span));
                        Ok(())
                    }
                }
            }
            ExprKind::AssignOp { op, lhs, rhs } => {
                let rhs_js = self.expr(rhs, out)?;
                let target = self.assignee(lhs)?;
                let ty = self.thir[lhs].ty;
                let current = target.clone().or_at(self.js_span(self.thir[lhs].span));
                let value = self.binary(assign_op(op), current, rhs_js, ty, expr.span)?.or_at(span);
                out.push(StmtKind::Assign(target, value).at(span));
                Ok(())
            }
            _ => {
                let value = match (dest, self.place(e)) {
                    // Returning a place hands its value over without a copy:
                    // every local dies here, so nothing is left to share it.
                    (Dest::Return, Some((place, _))) => place.or_at(span),
                    _ => self.expr(e, out)?,
                };
                match dest {
                    Dest::Return => out.push(StmtKind::Return(Some(value)).at(span)),
                    Dest::Assign(name) => out.push(StmtKind::Assign(Expr::var(name), value).at(span)),
                    Dest::Discard if value.has_effects() => out.push(StmtKind::Expr(value).at(span)),
                    Dest::Discard => {}
                }
                Ok(())
            }
        }
    }

    fn block(&mut self, block: BlockId, dest: &Dest, out: &mut Vec<Stmt>) -> R<()> {
        self.block_stmts(block, out)?;
        match self.thir[block].expr {
            Some(tail) => self.stmt(tail, dest, out),
            None => Ok(()),
        }
    }

    /// A block's statements, without its tail expression.
    fn block_stmts(&mut self, block: BlockId, out: &mut Vec<Stmt>) -> R<()> {
        let block = &self.thir[block];
        if block.targeted_by_break {
            return Err(self.unsupported(block.span, "labeled blocks"));
        }
        // Every local gets a unique JS name, so a Rust block needs no JS
        // block of its own: its statements go straight into `out`.
        for &stmt in &block.stmts {
            match &self.thir[stmt].kind {
                thir::StmtKind::Expr { expr, .. } => self.stmt(*expr, &Dest::Discard, out)?,
                thir::StmtKind::Let { pattern, initializer, else_block, span, .. } => {
                    if else_block.is_some() {
                        return Err(self.unsupported(*span, "`let ... else`"));
                    }
                    self.lower_let(pattern, *initializer, *span, out)?;
                }
            }
        }
        Ok(())
    }

    fn lower_let(
        &mut self,
        pat: &Pat<'tcx>,
        init: Option<ExprId>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let span = self.js_span(span);
        match &pat.kind {
            PatKind::Binding { name, var, mode, subpattern: None, ty, .. } => {
                self.check_by_value(*mode, pat.span)?;
                self.check_value_ty(*ty, pat.span)?;
                let mutable = mode.1 == Mutability::Mut;
                match init {
                    // Only control flow needs `let x;` and then assignments in
                    // its branches. Anything else (a closure, say) computes its
                    // statements first and then has a value.
                    Some(init) if self.is_simple(init) || !self.is_control_flow(init) => {
                        let value = self.expr(init, out)?;
                        let name = self.bind(*var, name.as_str(), mutable);
                        let kind = if mutable { StmtKind::Let(name, Some(value)) } else { StmtKind::Const(name, value) };
                        out.push(kind.at(span));
                    }
                    Some(init) => {
                        let name = self.bind(*var, name.as_str(), mutable);
                        out.push(StmtKind::Let(name.clone(), None).at(span));
                        self.stmt(init, &Dest::Assign(name), out)?;
                    }
                    None => {
                        let name = self.bind(*var, name.as_str(), mutable);
                        out.push(StmtKind::Let(name, None).at(span));
                    }
                }
                Ok(())
            }
            PatKind::Wild => match init {
                Some(init) => self.stmt(init, &Dest::Discard, out),
                None => Ok(()),
            },
            // `let (q, r) = divmod(a, b);`
            _ => {
                let Some(init) = init else {
                    return Err(self.unsupported(pat.span, "this `let` pattern without a value"));
                };
                let (subject, stable) = self.subject(init, "tmp", out)?;
                self.destructure(pat, subject, stable, out)
            }
        }
    }

    /// Bind the variables of an irrefutable pattern to the parts of `subject`.
    fn destructure(&mut self, pat: &Pat<'tcx>, subject: Expr, stable: bool, out: &mut Vec<Stmt>) -> R<()> {
        let mut bindings = Vec::new();
        if self.pattern_test(pat, &subject, &mut bindings)?.is_some() {
            return Err(self.unsupported(pat.span, "this refutable pattern"));
        }
        self.bind_all(bindings, stable, self.js_span(pat.span), out);
        Ok(())
    }

    /// Where a `match` or `let` finds the value it takes apart, and whether
    /// that stays unchanged while the pattern's variables live.
    ///
    /// A place is used where it is, and may be stable (see `stable_place`).
    /// A tuple of stable places (`match (a, b)`) is used without building
    /// the array. Anything else is computed once into a `const` named `base`,
    /// which is stable: no Rust variable can move or change it.
    fn subject(&mut self, e: ExprId, base: &str, out: &mut Vec<Stmt>) -> R<(Expr, bool)> {
        if let Some(place) = self.stable_place(e) {
            return Ok((place, true));
        }
        if let Some((place, _)) = self.place(e) {
            return Ok((place, false));
        }
        if let ExprKind::Tuple { ref fields } = self.thir[self.strip(e)].kind
            && !fields.is_empty()
        {
            let places: Option<Vec<Expr>> = fields.iter().map(|&f| self.stable_place(f)).collect();
            if let Some(places) = places {
                return Ok((Expr::array(places), true));
            }
        }
        let value = self.expr(e, out)?;
        let name = self.fresh(base);
        out.push(StmtKind::Const(name.clone(), value).at(self.js_span(self.thir[e].span)));
        Ok((Expr::var(&name), true))
    }

    /// Give a pattern's variables their JS meaning. Immutable ones bound into
    /// a stable subject just name the place they matched, as ReScript does:
    /// `P { x, y } => x + y` becomes `p.x + p.y`. The rest get a variable
    /// holding their own value.
    fn bind_all(&mut self, bindings: Vec<Binding<'tcx>>, stable: bool, span: js::Span, out: &mut Vec<Stmt>) {
        for b in bindings {
            if stable && !b.mutable {
                self.vars.insert(b.var, Var { place: b.place, mutable: false, depth: self.loops.len() });
                continue;
            }
            let value = self.copy_if_needed(b.place, b.ty).or_at(span);
            let name = self.bind(b.var, &b.name, b.mutable);
            let kind = if b.mutable { StmtKind::Let(name, Some(value)) } else { StmtKind::Const(name, value) };
            out.push(kind.at(span));
        }
    }

    fn lower_loop(
        &mut self,
        scope: region::Scope,
        hir_id: HirId,
        body: ExprId,
        dest: &Dest,
        span: js::Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let label_base = match self.tcx.hir_expect_expr(hir_id).kind {
            hir::ExprKind::Loop(_, Some(label), ..) => label.ident.name.as_str().trim_start_matches('\'').to_string(),
            _ => "loop".to_string(),
        };
        self.loops.push(Loop { scope, label_base, label: None, dest: dest.clone() });

        let mut body_out = Vec::new();
        // `while c { .. }` reaches us desugared as `loop { if c { .. } else { break } }`.
        // Put the `while` back.
        let cond = match self.as_while(body, scope) {
            Some((cond, then)) => {
                let cond = self.expr(cond, &mut body_out)?; // simple, so no statements
                self.stmt(then, &Dest::Discard, &mut body_out)?;
                cond
            }
            None => {
                self.stmt(body, &Dest::Discard, &mut body_out)?;
                Expr::bool(true)
            }
        };

        let label = self.loops.pop().unwrap().label;
        out.push(StmtKind::While { label, cond, body: body_out }.at(span));
        Ok(())
    }

    /// Recognize the `while` desugaring; returns `(cond, body)`.
    fn as_while(&self, body: ExprId, scope: region::Scope) -> Option<(ExprId, ExprId)> {
        let ExprKind::Block { block } = self.thir[self.strip(body)].kind else { return None };
        let block = &self.thir[block];
        let (true, Some(tail)) = (block.stmts.is_empty(), block.expr) else { return None };
        let ExprKind::If { cond, then, else_opt: Some(els), .. } = self.thir[self.strip(tail)].kind
        else {
            return None;
        };
        let ExprKind::Block { block: els } = self.thir[self.strip(els)].kind else { return None };
        let els = &self.thir[els];
        let ([stmt], None) = (&*els.stmts, els.expr) else { return None };
        let thir::StmtKind::Expr { expr, .. } = self.thir[*stmt].kind else { return None };
        let ExprKind::Break { label, value: None } = self.thir[self.strip(expr)].kind else {
            return None;
        };
        (label == scope && self.is_simple(cond)).then_some((cond, then))
    }

    fn lower_match(
        &mut self,
        scrutinee: ExprId,
        arms: &[ArmId],
        dest: &Dest,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        // Evaluate the scrutinee once, unless it's a place that can be
        // tested where it is.
        let (subject, stable) = self.subject(scrutinee, "match", out)?;

        let mut chain: Vec<(Option<Expr>, Vec<Stmt>, js::Span)> = Vec::new();
        for (i, &arm_id) in arms.iter().enumerate() {
            let arm = &self.thir[arm_id];
            let arm_span = self.js_span(arm.span);
            let pat_span = self.js_span(arm.pattern.span);
            let mut bindings = Vec::new();
            let mut test = self
                .pattern_test(&arm.pattern, &subject, &mut bindings)?
                .map(|t| t.or_at(pat_span));

            // A guard is tested before the arm's body, where the variables
            // with their own `const` are declared. Only named places work there.
            if arm.guard.is_some() && bindings.iter().any(|b| !stable || b.mutable) {
                return Err(self.unsupported(arm.pattern.span, "this binding in a guarded arm"));
            }
            let mut body = Vec::new();
            self.bind_all(bindings, stable, pat_span, &mut body);
            if let Some(guard) = arm.guard {
                if !self.is_simple(guard) {
                    return Err(self.unsupported(self.thir[guard].span, "this guard"));
                }
                let guard = self.expr(guard, out)?;
                test = Some(match test {
                    Some(t) => Expr::bin(Op::And, t, guard),
                    None => guard,
                });
            }
            // Rust checked the match is exhaustive, so if we reach the last
            // unguarded arm, it matches. No need to test it.
            if i == arms.len() - 1 && arm.guard.is_none() {
                test = None;
            }
            self.stmt(arm.body, dest, &mut body)?;
            let done = test.is_none();
            chain.push((test, body, arm_span));
            if done {
                break; // Later arms are unreachable.
            }
        }

        // Fold into `if (..) {..} else if (..) {..} else {..}`.
        let mut rest: Option<Vec<Stmt>> = None;
        for (test, body, span) in chain.into_iter().rev() {
            rest = Some(match test {
                Some(t) => vec![StmtKind::If(t, body, rest).at(span)],
                None => body,
            });
        }
        out.extend(rest.unwrap_or_default());
        Ok(())
    }

    /// A JS boolean test for "`subject` matches `pat`" (`None`: always matches).
    fn pattern_test(
        &mut self,
        pat: &Pat<'tcx>,
        subject: &Expr,
        bindings: &mut Vec<Binding<'tcx>>,
    ) -> R<Option<Expr>> {
        match &pat.kind {
            PatKind::Wild => Ok(None),
            PatKind::Binding { name, var, mode, subpattern: None, ty, .. } => {
                self.check_by_value(*mode, pat.span)?;
                bindings.push(Binding {
                    var: *var,
                    name: name.to_string(),
                    mutable: mode.1 == Mutability::Mut,
                    place: subject.clone(),
                    ty: *ty,
                });
                Ok(None)
            }
            PatKind::Constant { value } => {
                let value = self.const_value(*value, pat.span)?;
                Ok(Some(Expr::bin(Op::Eq, subject.clone(), value)))
            }
            PatKind::Variant { adt_def, variant_index, subpatterns, .. } if subpatterns.is_empty() => {
                let name = adt_def.variant(*variant_index).name.to_string();
                Ok(Some(Expr::bin(Op::Eq, subject.clone(), Expr::str(name))))
            }
            // A struct or tuple: every field must match.
            PatKind::Leaf { subpatterns } => {
                let mut tests = Vec::new();
                for field in subpatterns {
                    let part = self.project(subject.clone(), pat.ty, field.field.as_usize());
                    tests.extend(self.pattern_test(&field.pattern, &part, bindings)?);
                }
                Ok(tests.into_iter().reduce(|a, b| Expr::bin(Op::And, a, b)))
            }
            PatKind::Or { pats } => {
                let before = bindings.len();
                let mut tests = Vec::new();
                for p in pats {
                    match self.pattern_test(p, subject, bindings)? {
                        Some(t) => tests.push(t),
                        None => return Ok(None),
                    }
                }
                if bindings.len() != before {
                    return Err(self.unsupported(pat.span, "bindings inside `|` patterns"));
                }
                Ok(tests.into_iter().reduce(|a, b| Expr::bin(Op::Or, a, b)))
            }
            _ => Err(self.unsupported(pat.span, "this pattern")),
        }
    }

    // ── Expression mode ─────────────────────────────────────────────────

    /// Lower `e` to a JS expression. Any statements it needs first (a
    /// block's `let`s, a `match` computing a temporary) are pushed to `out`.
    ///
    /// The result carries `e`'s span, unless a more precise one was set
    /// deeper down (a `Scope` passes its inner expression through, say).
    fn expr(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        let span = self.js_span(self.thir[e].span);
        Ok(self.expr_inner(e, out)?.or_at(span))
    }

    fn expr_inner(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        let expr = &self.thir[e];
        let span = expr.span;
        let js_span = self.js_span(span);
        let ty = expr.ty;
        match expr.kind {
            ExprKind::Scope { value, .. } if !matches!(self.thir[value].kind, ExprKind::Loop { .. }) => {
                self.expr(value, out)
            }
            ExprKind::Use { source }
            | ExprKind::ValueTypeAscription { source, .. }
            | ExprKind::PlaceTypeAscription { source, .. } => self.expr(source, out),
            ExprKind::Block { block } if !self.thir[block].targeted_by_break => {
                self.block_stmts(block, out)?;
                match self.thir[block].expr {
                    Some(tail) => self.expr(tail, out),
                    None => Ok(Expr::undefined()),
                }
            }
            ExprKind::Literal { lit, neg } => self.literal(&lit.node, neg, ty, span),
            ExprKind::NonHirLiteral { lit, .. } => {
                if ty.is_bool() {
                    return Ok(Expr::bool(lit.to_bits_unchecked() != 0));
                }
                let num = self.num(ty, span)?;
                Ok(num_literal(lit.to_bits_unchecked(), num))
            }
            ExprKind::VarRef { .. }
            | ExprKind::UpvarRef { .. }
            | ExprKind::Field { .. }
            | ExprKind::Deref { .. }
            | ExprKind::StaticRef { .. } => self.read(e, out),
            // A shared reference is the value it points to (ADR 0023): JS
            // shares objects anyway, and nothing can change through it.
            ExprKind::Borrow { borrow_kind: BorrowKind::Shared, arg } => match self.place(arg) {
                Some((place, _)) => Ok(place),
                None => self.expr(arg, out),
            },
            ExprKind::Borrow { .. } => Err(self.unsupported(span, "`&mut` references")),
            // `Box<closure>` to `Box<dyn FnMut()>`: the same JS function.
            ExprKind::PointerCoercion { cast: PointerCoercion::Unsize, source, .. } => self.expr(source, out),
            ExprKind::Closure(ref closure) => self.closure(closure, out),
            ExprKind::Tuple { ref fields } if fields.is_empty() => Ok(Expr::undefined()),
            ExprKind::Tuple { ref fields } => Ok(Expr::array(self.operands(fields, out)?)),
            ExprKind::Adt(ref adt) => self.adt(adt, ty, span, out),
            ExprKind::Binary { op, lhs, rhs } => {
                let [l, r] = self.operands(&[lhs, rhs], out)?.try_into().ok().unwrap();
                self.binary(op, l, r, self.thir[lhs].ty, span)
            }
            ExprKind::LogicalOp { op, lhs, rhs } => {
                let l = self.expr(lhs, out)?;
                let js_op = match op {
                    LogicalOp::And => Op::And,
                    LogicalOp::Or => Op::Or,
                };
                if self.is_simple(rhs) {
                    let r = self.expr(rhs, out)?;
                    return Ok(Expr::bin(js_op, l, r));
                }
                // `a && { .. }`: only run the right side's statements if needed.
                let tmp = self.fresh("tmp");
                out.push(StmtKind::Let(tmp.clone(), Some(l)).at(js_span));
                let mut rhs_out = Vec::new();
                self.stmt(rhs, &Dest::Assign(tmp.clone()), &mut rhs_out)?;
                let test = match op {
                    LogicalOp::And => Expr::var(&tmp),
                    LogicalOp::Or => Expr::unary(UnaryOp::Not, Expr::var(&tmp)),
                };
                out.push(StmtKind::If(test, rhs_out, None).at(js_span));
                Ok(Expr::var(&tmp))
            }
            ExprKind::Unary { op, arg } => {
                let a = self.expr(arg, out)?;
                self.unary(op, a, ty, span)
            }
            ExprKind::Cast { source } => {
                let v = self.expr(source, out)?;
                self.cast(v, self.thir[source].ty, ty, span)
            }
            ExprKind::Call { fun, ref args, .. } => self.call(fun, args, span, out),
            ExprKind::If { cond, then, else_opt: Some(els), .. }
                if self.is_simple(then) && self.is_simple(els) =>
            {
                let c = self.expr(cond, out)?;
                let t = self.expr(then, out)?;
                let f = self.expr(els, out)?;
                Ok(Expr::cond(c, t, f))
            }
            // Control flow: run it as statements, then read the result.
            ExprKind::Scope { .. }
            | ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::Block { .. }
            | ExprKind::NeverToAny { .. }
            | ExprKind::Return { .. }
            | ExprKind::Break { .. }
            | ExprKind::Continue { .. }
            | ExprKind::Assign { .. }
            | ExprKind::AssignOp { .. } => {
                if ty.is_unit() || ty.is_never() {
                    self.stmt(e, &Dest::Discard, out)?;
                    return Ok(Expr::undefined());
                }
                let tmp = self.fresh("tmp");
                out.push(StmtKind::Let(tmp.clone(), None).at(js_span));
                self.stmt(e, &Dest::Assign(tmp.clone()), out)?;
                Ok(Expr::var(&tmp))
            }
            _ => Err(self.unsupported(span, "this expression")),
        }
    }

    /// Lower operands left to right. If a later operand needs statements,
    /// earlier ones are saved in temporaries first, so Rust's evaluation
    /// order is kept.
    fn operands(&mut self, list: &[ExprId], out: &mut Vec<Stmt>) -> R<Vec<Expr>> {
        let last_complex = list.iter().rposition(|&e| !self.is_simple(e));
        let mut values = Vec::new();
        for (i, &e) in list.iter().enumerate() {
            let v = self.expr(e, out)?;
            // Constants, and places that can't change, read the same later.
            let settled = v.is_constant() || self.stable_place(e).is_some();
            if last_complex.is_some_and(|k| i < k) && !settled {
                let tmp = self.fresh("tmp");
                let span = v.span;
                out.push(StmtKind::Const(tmp.clone(), v).at(span));
                values.push(Expr::var(&tmp));
            } else {
                values.push(v);
            }
        }
        Ok(values)
    }

    /// Does calling `fun` become an assignment statement?
    fn is_assignment_call(&self, fun: ExprId) -> bool {
        if matches!(self.std_fn(fun), Some(Std::CellSet)) {
            return true;
        }
        let &ty::FnDef(def_id, _) = self.thir[self.strip(fun)].ty.kind() else { return false };
        self.tcx.is_foreign_item(def_id) && matches!(js_form(self.tcx, def_id), JsForm::Set(_))
    }

    /// Is `e` a Rust expression that JS can only write as statements?
    fn is_control_flow(&self, e: ExprId) -> bool {
        matches!(
            self.thir[self.strip(e)].kind,
            ExprKind::If { .. } | ExprKind::Match { .. } | ExprKind::Block { .. } | ExprKind::Loop { .. }
        )
    }

    /// Can `e` become a JS expression with no statements before it?
    fn is_simple(&self, e: ExprId) -> bool {
        match self.thir[e].kind {
            ExprKind::Scope { value, .. } => {
                !matches!(self.thir[value].kind, ExprKind::Loop { .. }) && self.is_simple(value)
            }
            ExprKind::Use { source }
            | ExprKind::ValueTypeAscription { source, .. }
            | ExprKind::PlaceTypeAscription { source, .. }
            | ExprKind::Cast { source }
            | ExprKind::PointerCoercion { source, .. }
            | ExprKind::Borrow { arg: source, .. }
            | ExprKind::Deref { arg: source }
            | ExprKind::Unary { arg: source, .. } => self.is_simple(source),
            ExprKind::Literal { .. }
            | ExprKind::NonHirLiteral { .. }
            | ExprKind::VarRef { .. }
            | ExprKind::UpvarRef { .. }
            | ExprKind::StaticRef { .. }
            | ExprKind::ZstLiteral { .. } => true,
            ExprKind::Field { lhs, .. } => self.is_simple(lhs),
            // Its body's statements go inside the arrow; only snapshots come first.
            ExprKind::Closure(ref closure) => closure.upvars.iter().all(|&u| !self.needs_snapshot(u)),
            ExprKind::Tuple { ref fields } => fields.iter().all(|&f| self.is_simple(f)),
            ExprKind::Adt(ref adt) => {
                let base_simple = match &adt.base {
                    AdtExprBase::Base(fru) => self.is_simple(fru.base),
                    _ => true,
                };
                // Fields written out of declaration order may need temporaries.
                base_simple
                    && adt.fields.is_sorted_by_key(|f| f.name)
                    && adt.fields.iter().all(|f| self.is_simple(f.expr))
            }
            ExprKind::Binary { lhs, rhs, .. } | ExprKind::LogicalOp { lhs, rhs, .. } => {
                self.is_simple(lhs) && self.is_simple(rhs)
            }
            // `cell.set(v)` and JS property setters are assignment statements.
            ExprKind::Call { fun, ref args, .. } => {
                !self.is_assignment_call(fun)
                    && self.is_simple(fun)
                    && args.iter().all(|&a| self.is_simple(a))
            }
            ExprKind::If { cond, then, else_opt: Some(els), .. } => {
                self.is_simple(cond) && self.is_simple(then) && self.is_simple(els)
            }
            ExprKind::Block { block } => {
                let block = &self.thir[block];
                !block.targeted_by_break
                    && block.stmts.is_empty()
                    && block.expr.is_none_or(|t| self.is_simple(t))
            }
            _ => false,
        }
    }

    // ── Operators ───────────────────────────────────────────────────────

    fn binary(&mut self, op: BinOp, l: Expr, r: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let comparison = match op {
            BinOp::Eq => Some(Op::Eq),
            BinOp::Ne => Some(Op::Ne),
            BinOp::Lt => Some(Op::Lt),
            BinOp::Le => Some(Op::Le),
            BinOp::Gt => Some(Op::Gt),
            BinOp::Ge => Some(Op::Ge),
            _ => None,
        };
        if let Some(js_op) = comparison {
            return Ok(Expr::bin(js_op, l, r));
        }

        if ty.is_bool() {
            // `&`, `|`, `^` on bools evaluate both sides and give a bool.
            return match op {
                BinOp::BitXor => Ok(Expr::bin(Op::Ne, l, r)),
                BinOp::BitAnd => Ok(Expr::unary(UnaryOp::Not, Expr::unary(UnaryOp::Not, Expr::bin(Op::BitAnd, l, r)))),
                BinOp::BitOr => Ok(Expr::unary(UnaryOp::Not, Expr::unary(UnaryOp::Not, Expr::bin(Op::BitOr, l, r)))),
                _ => Err(self.unsupported(span, "this operator on `bool`")),
            };
        }

        let num = self.num(ty, span)?;
        if num == Num::F64 {
            let js_op = match op {
                BinOp::Add => Op::Add,
                BinOp::Sub => Op::Sub,
                BinOp::Mul => Op::Mul,
                BinOp::Div => Op::Div,
                BinOp::Rem => Op::Rem,
                _ => return Err(self.unsupported(span, "this operator on `f64`")),
            };
            return Ok(Expr::bin(js_op, l, r));
        }

        // Integers: compute exactly in JS, then wrap back into range.
        Ok(match op {
            BinOp::Add => num.wrap(Expr::bin(Op::Add, l, r)),
            BinOp::Sub => num.wrap(Expr::bin(Op::Sub, l, r)),
            // A 32-bit product can exceed 2^53 and lose bits; `Math.imul` can't.
            BinOp::Mul if num.bits() == 32 => {
                let product = Expr::call(Expr::member(Expr::var("Math"), "imul"), vec![l, r]);
                if num.signed() { product } else { num.wrap(product) }
            }
            BinOp::Mul => num.wrap(Expr::bin(Op::Mul, l, r)),
            BinOp::Div | BinOp::Rem => {
                let (js_op, helper, name) = match op {
                    BinOp::Div => (Op::Div, Helper::Div, "$div"),
                    _ => (Op::Rem, Helper::Rem, "$rem"),
                };
                // A literal divisor that can't panic stays inline: `a / 3 | 0`.
                let safe = r.as_int().is_some_and(|d| d != 0 && !(num.signed() && d == -1));
                let quotient = if safe {
                    Expr::bin(js_op, l, r)
                } else {
                    self.runtime.insert(helper);
                    let mut args = vec![l, r];
                    if num.signed() {
                        args.push(Expr::int(num.range().0));
                    }
                    Expr::call(Expr::var(name), args)
                };
                // The remainder of in-range integers is already in range.
                if op == BinOp::Rem && safe { quotient } else { num.wrap(quotient) }
            }
            BinOp::BitAnd => self.bitwise(Op::BitAnd, l, r, num),
            BinOp::BitOr => self.bitwise(Op::BitOr, l, r, num),
            BinOp::BitXor => self.bitwise(Op::BitXor, l, r, num),
            // Rust (without overflow checks) masks the shift amount to the
            // type's width. JS masks to 32, which is only right for 32 bits.
            BinOp::Shl if num.bits() == 32 => num.wrap(Expr::bin(Op::Shl, l, r)),
            BinOp::Shl => num.wrap(Expr::bin(Op::Shl, l, mask_shift(r, num))),
            BinOp::Shr => {
                let js_op = if num.signed() { Op::Shr } else { Op::UShr };
                let r = if num.bits() == 32 { r } else { mask_shift(r, num) };
                Expr::bin(js_op, l, r)
            }
            _ => return Err(self.unsupported(span, "this operator")),
        })
    }

    fn bitwise(&self, op: Op, l: Expr, r: Expr, num: Num) -> Expr {
        // JS bitwise ops return signed 32-bit results; only u32 needs fixing.
        let e = Expr::bin(op, l, r);
        if num == Num::U32 { num.wrap(e) } else { e }
    }

    fn unary(&mut self, op: UnOp, a: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        match op {
            UnOp::Not if ty.is_bool() => Ok(Expr::unary(UnaryOp::Not, a)),
            UnOp::Not => {
                let num = self.num(ty, span)?;
                if num == Num::F64 {
                    return Err(self.unsupported(span, "`!` on `f64`"));
                }
                let e = Expr::unary(UnaryOp::BitNot, a);
                Ok(if num.signed() { e } else { num.wrap(e) })
            }
            UnOp::Neg => {
                let num = self.num(ty, span)?;
                // `-x` of a literal is just a negative literal.
                if let Some(n) = a.as_int() {
                    return Ok(Expr::int(-n));
                }
                Ok(num.wrap(Expr::unary(UnaryOp::Neg, a)))
            }
            UnOp::PtrMetadata => Err(self.unsupported(span, "pointer metadata")),
        }
    }

    fn cast(&mut self, v: Expr, from: Ty<'tcx>, to: Ty<'tcx>, span: Span) -> R<Expr> {
        let target = self.num(to, span)?;
        if from.is_bool() && target != Num::F64 {
            return Ok(Expr::cond(v, Expr::num(1), Expr::num(0)));
        }
        let source = self.num(from, span)?;
        match (source, target) {
            (Num::F64, Num::F64) => Ok(v),
            // `as` from float to int saturates; we don't do that yet.
            (Num::F64, _) => Err(self.unsupported(span, "casting `f64` to an integer")),
            // Every integer we support fits exactly in an f64.
            (_, Num::F64) => Ok(v),
            _ => {
                let (lo, hi) = source.range();
                let (tlo, thi) = target.range();
                Ok(if tlo <= lo && hi <= thi { v } else { target.wrap(v) })
            }
        }
    }

    // ── Leaves ──────────────────────────────────────────────────────────

    fn literal(&self, lit: &LitKind, neg: bool, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        match *lit {
            LitKind::Bool(b) => Ok(Expr::bool(b)),
            LitKind::Str(s, _) => Ok(Expr::str(s.as_str())),
            LitKind::Int(n, _) => {
                self.num(ty, span)?;
                let n = n.get() as i128;
                Ok(Expr::int(if neg { -n } else { n }))
            }
            LitKind::Float(sym, _) if Num::of(ty) == Some(Num::F64) => {
                let x: f64 = sym.as_str().replace('_', "").parse().expect("rustc validated the literal");
                Ok(Expr::num(if neg { -x } else { x }))
            }
            _ => Err(self.unsupported(span, "this literal")),
        }
    }

    fn const_value(&self, value: ty::Value<'tcx>, span: Span) -> R<Expr> {
        if let Some(b) = value.try_to_bool() {
            return Ok(Expr::bool(b));
        }
        let (Some(num), Some(leaf)) = (Num::of(value.ty), value.try_to_leaf()) else {
            return Err(self.unsupported(span, "this constant pattern"));
        };
        Ok(num_literal(leaf.to_bits_unchecked(), num))
    }

    /// A call to one of our functions (`f`, or `alias.f` in another module),
    /// to JS (ADR 0021), or to one of the std functions rust-js knows (ADR 0023).
    fn call(&mut self, fun: ExprId, args: &[ExprId], span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let fun_span = self.js_span(self.thir[fun].span);
        let f = &self.thir[self.strip(fun)];
        let (ExprKind::ZstLiteral { .. }, &ty::FnDef(def_id, generic_args)) = (&f.kind, f.ty.kind()) else {
            return Err(self.unsupported(f.span, "calling this"));
        };
        if let Some(target) = self.fns.get(&def_id) {
            let callee = if target.module == self.module {
                Expr::var(&target.name)
            } else {
                Expr::member(Expr::var(&self.aliases[&target.module]), target.name.clone())
            };
            let args = self.operands(args, out)?;
            return Ok(Expr::call(callee.or_at(fun_span), args));
        }
        if self.tcx.is_foreign_item(def_id) {
            let mut args = self.operands(args, out)?;
            let this = is_method(self.tcx, def_id).then(|| args.remove(0));
            return Ok(match (js_form(self.tcx, def_id), this) {
                (JsForm::Call(name), Some(this)) => Expr::call(Expr::member(this, name).or_at(fun_span), args),
                (JsForm::Call(name), None) => Expr::call(global(&name).or_at(fun_span), args),
                (JsForm::New(name), None) => Expr::new_(global(&name).or_at(fun_span), args),
                (JsForm::Get(name), Some(this)) if args.is_empty() => Expr::member(this, name),
                (JsForm::Set(name), Some(this)) if args.len() == 1 => {
                    let value = args.remove(0);
                    out.push(StmtKind::Assign(Expr::member(this, name), value).at(self.js_span(span)));
                    Expr::undefined()
                }
                (JsForm::This, Some(this)) if args.is_empty() => this,
                _ => {
                    let what = format!("the `#[link_name]` of `{}` with this signature", self.tcx.def_path_str(def_id));
                    return Err(self.unsupported(self.thir[fun].span, &what));
                }
            });
        }
        // Calling a closure, `f(a, b)`, is `Fn::call(&f, (a, b))`: in JS, `f(a, b)`.
        if let Some(fn_trait) = self.tcx.trait_of_assoc(def_id)
            && self.tcx.fn_trait_kind_from_def_id(fn_trait).is_some()
        {
            let [callee, ExprKind::Tuple { fields }] = [args[0], args[1]].map(|a| &self.thir[self.strip(a)].kind)
            else {
                return Err(self.unsupported(span, "this closure call"));
            };
            let callee = match *callee {
                // `&f` or `&mut f`: the closure itself.
                ExprKind::Borrow { arg, .. } => arg,
                _ => args[0],
            };
            let mut list = vec![callee];
            list.extend(fields.iter().copied());
            let mut values = self.operands(&list, out)?;
            let callee = values.remove(0);
            return Ok(Expr::call(callee, values));
        }
        let Some(known) = self.std_fn(fun) else {
            let path = self.tcx.def_path_str(def_id);
            return Err(self.unsupported(self.thir[fun].span, &format!("calling `{path}`")));
        };
        let mut values = self.operands(args, out)?.into_iter();
        let first = values.next().expect("every known std function takes an argument");
        Ok(match known {
            // An `Rc` is the JS reference itself: the garbage collector does
            // its counting, so a clone is the same object.
            Std::BoxNew | Std::RcNew | Std::RcClone | Std::Deref => first,
            // A `Cell` is `{ value }`, so everyone sharing it sees a `set`.
            Std::CellNew => Expr::object(vec![Prop::Field("value".into(), first)]),
            Std::Concat => Expr::bin(Op::Add, first, values.next().expect("`+` takes two operands")),
            Std::CellGet => self.copy_if_needed(Expr::member(first, "value"), generic_args.type_at(0)),
            Std::CellSet => {
                let value = values.next().expect("`Cell::set` takes a value");
                out.push(StmtKind::Assign(Expr::member(first, "value"), value).at(self.js_span(span)));
                Expr::undefined()
            }
            Std::ToString => {
                let ty = generic_args.type_at(0);
                if ty.is_str() || self.is_lang_adt(ty, LangItem::String) {
                    first
                } else if ty.is_bool() || Num::of(ty).is_some_and(|n| n != Num::F64) {
                    Expr::call(Expr::var("String"), vec![first])
                } else {
                    return Err(self.unsupported(span, &format!("`to_string` on `{ty}`")));
                }
            }
        })
    }

    /// Which std function `fun` is, if rust-js knows what it means in JS.
    fn std_fn(&self, fun: ExprId) -> Option<Std> {
        let tcx = self.tcx;
        let &ty::FnDef(def_id, args) = self.thir[self.strip(fun)].ty.kind() else { return None };
        let diagnostic = |name: &str| tcx.is_diagnostic_item(Symbol::intern(name), def_id);
        if diagnostic("box_new") {
            return Some(Std::BoxNew);
        }
        if diagnostic("to_string_method") {
            return Some(Std::ToString);
        }
        let self_ty = args.types().next();
        if diagnostic("deref_method") {
            let ty = self_ty?;
            // A JS object's `Deref` goes to the interface it inherits from:
            // the same object (ADR 0024).
            let identity = self.is_lang_adt(ty, LangItem::String) || self.is_std_adt(ty, sym::Rc) || self.is_js_object(ty);
            return identity.then_some(Std::Deref);
        }
        if tcx.trait_of_assoc(def_id).is_some_and(|t| tcx.is_lang_item(t, LangItem::Add)) {
            return self.is_lang_adt(self_ty?, LangItem::String).then_some(Std::Concat);
        }
        if tcx.is_lang_item(def_id, LangItem::CloneFn) {
            return self.is_std_adt(self_ty?, sym::Rc).then_some(Std::RcClone);
        }
        let owner = tcx.type_of(tcx.inherent_impl_of_assoc(def_id)?).instantiate_identity();
        match tcx.item_name(def_id).as_str() {
            "new" if self.is_std_adt(owner, sym::Rc) => Some(Std::RcNew),
            "new" if self.is_std_adt(owner, sym::Cell) => Some(Std::CellNew),
            "get" if self.is_std_adt(owner, sym::Cell) => Some(Std::CellGet),
            "set" if self.is_std_adt(owner, sym::Cell) => Some(Std::CellSet),
            _ => None,
        }
    }

    fn is_std_adt(&self, ty: Ty<'tcx>, name: Symbol) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.is_diagnostic_item(name, adt.did()))
    }

    fn is_lang_adt(&self, ty: Ty<'tcx>, item: LangItem) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.is_lang_item(adt.did(), item))
    }

    /// std types that aren't plain structs in JS: `String` is a JS string,
    /// `Box<T>` and `Rc<T>` are just `T`, `Cell<T>` is `{ value }`.
    fn is_std_wrapper(&self, ty: Ty<'tcx>) -> bool {
        ty.is_box()
            || self.is_lang_adt(ty, LangItem::String)
            || self.is_std_adt(ty, sym::Rc)
            || self.is_std_adt(ty, sym::Cell)
    }

    /// A struct that stands for a JS object, like `web::Element` (ADR 0024):
    /// its only field is `PhantomData` of an extern type. Rust never builds
    /// one; it only holds references to them, which are the JS objects.
    fn is_js_object(&self, ty: Ty<'tcx>) -> bool {
        let ty::Adt(adt, args) = ty.kind() else { return false };
        if !adt.is_struct() || adt.non_enum_variant().fields.len() != 1 {
            return false;
        }
        let field = adt.non_enum_variant().fields.iter().next().expect("one field").ty(self.tcx, args);
        matches!(field.kind(), ty::Adt(marker, marked) if marker.is_phantom_data()
            && marked.types().next().is_some_and(|t| matches!(t.kind(), ty::Foreign(_))))
    }

    // ── Closures (ADR 0022) ─────────────────────────────────────────────

    /// A closure is an arrow function, lowered right where it's created.
    ///
    /// JS closures capture *variables*, which is what a Rust capture by
    /// reference means, and the borrow checker has made sure nothing else
    /// uses them meanwhile. A capture by value is a copy: for an immutable
    /// variable that's the same thing, so only mutable ones get a snapshot.
    fn closure(&mut self, closure: &thir::ClosureExpr<'tcx>, out: &mut Vec<Stmt>) -> R<Expr> {
        let body: &'a Body<'tcx> = self.closures[&closure.closure_id];
        let mut shadowed = Vec::new();
        for &upvar in closure.upvars.iter() {
            if !self.needs_snapshot(upvar) {
                continue;
            }
            // Since Rust 2021 a closure may capture part of a variable
            // (`p.x`), so the snapshot stands for that place.
            let span = self.thir[upvar].span;
            let Some(path) = self.place_path(upvar) else {
                return Err(self.unsupported(span, "capturing this place by value"));
            };
            let value = self.read(upvar, out)?;
            // Named after what it copies, from the Rust name: `n` gives `n$1`.
            let base = match self.place(upvar) {
                Some((Expr { kind: js::ExprKind::Member(_, field), .. }, _)) => field,
                Some((Expr { kind: js::ExprKind::Var(name), .. }, _)) => name,
                _ => "capture".to_string(),
            };
            let name = self.fresh(base.split('$').next().unwrap_or_default());
            out.push(StmtKind::Let(name.clone(), Some(value)).at(self.js_span(span)));
            let snapshot = Var { place: Expr::var(&name), mutable: true, depth: self.loops.len() };
            shadowed.push((path.clone(), self.captures.insert(path, snapshot)));
        }

        // Lower the body as if it were a function of its own, then come back.
        let thir = std::mem::replace(&mut self.thir, &body.thir);
        let loops = std::mem::take(&mut self.loops);
        let mut stmts = Vec::new();
        // The first parameter is the closure itself, which JS doesn't need.
        let params = self.lower_params(&body.thir.params.raw[1..], self.tcx.def_span(body.def_id), &mut stmts)?;
        let BodyTy::Fn(sig) = body.thir.body_type else { unreachable!("a closure body is a function") };
        let dest = if sig.output().is_unit() { Dest::Discard } else { Dest::Return };
        self.stmt(body.expr, &dest, &mut stmts)?;
        self.thir = thir;
        self.loops = loops;
        for (path, previous) in shadowed {
            match previous {
                Some(var) => self.captures.insert(path, var),
                None => self.captures.remove(&path),
            };
        }
        Ok(Expr::arrow(params, stmts))
    }

    /// A place as a variable and a path of fields, like `p.x` as `(p, [0])`.
    fn place_path(&self, e: ExprId) -> Option<(LocalVarId, Vec<usize>)> {
        match self.thir[self.strip(e)].kind {
            ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } => Some((id, Vec::new())),
            ExprKind::Field { lhs, name, .. } => {
                let (id, mut path) = self.place_path(lhs)?;
                path.push(name.as_usize());
                Some((id, path))
            }
            _ => None,
        }
    }

    /// Does capturing `upvar` need a snapshot? Only a by-value capture of a
    /// mutable variable does, and not when the capture is the variable's
    /// only use, outside any loop the variable isn't also in.
    fn needs_snapshot(&self, upvar: ExprId) -> bool {
        let u = self.strip(upvar);
        if matches!(self.thir[u].kind, ExprKind::Borrow { .. }) {
            return false;
        }
        let Some(var) = self.root_var(u).and_then(|id| self.vars.get(&id)) else { return false };
        if !var.mutable {
            return false;
        }
        let only_use = match self.thir[u].kind {
            ExprKind::VarRef { id } => {
                let uses = self.thir.exprs.iter().filter(|e| matches!(e.kind, ExprKind::VarRef { id: i } if i == id));
                uses.count() == 1 && var.depth == self.loops.len()
            }
            _ => false,
        };
        !only_use
    }

    /// The variable a place starts from.
    fn root_var(&self, e: ExprId) -> Option<LocalVarId> {
        match self.thir[self.strip(e)].kind {
            ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } => Some(id),
            ExprKind::Field { lhs, .. } | ExprKind::Deref { arg: lhs } => self.root_var(lhs),
            _ => None,
        }
    }

    // ── Structs and tuples (ADR 0020) ───────────────────────────────────

    /// A struct literal: `{ x: 1, y: 2 }`, or `[1, 2]` for a tuple struct.
    fn adt(&mut self, adt: &thir::AdtExpr<'tcx>, ty: Ty<'tcx>, span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let variant = adt.adt_def.variant(adt.variant_index);
        if adt.adt_def.is_enum() {
            if !is_fieldless_enum(adt.adt_def) {
                return Err(self.unsupported(span, "enums with fields"));
            }
            return Ok(Expr::str(variant.name.to_string()));
        }
        if adt.adt_def.is_union() {
            return Err(self.unsupported(span, "unions"));
        }
        // `struct Marker;` holds nothing, like `()`.
        if variant.ctor_kind() == Some(CtorKind::Const) {
            return Ok(Expr::undefined());
        }
        // `P { x, ..base }`: the fields not written come from `base`.
        let base = match &adt.base {
            AdtExprBase::None => None,
            AdtExprBase::Base(fru) => match self.place(fru.base) {
                Some((place, _)) => Some(place),
                None => return Err(self.unsupported(self.thir[fru.base].span, "`..` with this base")),
            },
            AdtExprBase::DefaultFields(_) => return Err(self.unsupported(span, "default field values")),
        };

        // Rust evaluates the fields in the order they're written. JS lists
        // them in declaration order, so every object of a type has the same
        // shape. If that reorders two calls, they go into `const`s first.
        let exprs: Vec<ExprId> = adt.fields.iter().map(|f| f.expr).collect();
        let mut values = self.operands(&exprs, out)?;
        let reordered = !adt.fields.is_sorted_by_key(|f| f.name);
        if reordered && values.iter().filter(|v| v.has_effects()).count() > 1 {
            for (field, value) in adt.fields.iter().zip(&mut values) {
                if value.has_effects() {
                    let name = self.fresh(variant.fields[field.name].name.as_str());
                    let v = std::mem::replace(value, Expr::var(&name));
                    let span = v.span;
                    out.push(StmtKind::Const(name, v).at(span));
                }
            }
        }
        let mut given: HashMap<usize, Expr> =
            adt.fields.iter().map(|f| f.name.as_usize()).zip(values).collect();

        let shape = self.shape(ty);
        let field_tys = match &shape {
            Shape::Object(fields) => fields.iter().map(|&(_, t)| t).collect(),
            Shape::Array(tys) => tys.clone(),
            Shape::Other => unreachable!("a struct with fields"),
        };
        let mut items = Vec::new();
        for (i, field_ty) in field_tys.into_iter().enumerate() {
            items.push(match (given.remove(&i), &base) {
                (Some(value), _) => value,
                (None, Some(base)) => self.copy_if_needed(self.project(base.clone(), ty, i), field_ty),
                (None, None) => unreachable!("rustc checked that every field is given"),
            });
        }
        Ok(match shape {
            Shape::Object(fields) => {
                Expr::object(fields.into_iter().zip(items).map(|((name, _), v)| Prop::Field(name, v)).collect())
            }
            _ => Expr::array(items),
        })
    }

    /// How a struct or tuple type looks in JS.
    fn shape(&self, ty: Ty<'tcx>) -> Shape<'tcx> {
        if self.is_std_wrapper(ty) || self.is_js_object(ty) {
            return Shape::Other;
        }
        match ty.kind() {
            ty::Tuple(tys) if !tys.is_empty() => Shape::Array(tys.to_vec()),
            ty::Adt(adt, args) if adt.is_struct() => {
                let variant = adt.non_enum_variant();
                let fields = variant.fields.iter().map(|f| (f.name.to_string(), f.ty(self.tcx, args)));
                match variant.ctor_kind() {
                    None => Shape::Object(fields.collect()),
                    Some(CtorKind::Fn) => Shape::Array(fields.map(|(_, ty)| ty).collect()),
                    Some(CtorKind::Const) => Shape::Other,
                }
            }
            _ => Shape::Other,
        }
    }

    /// Field `i` of a `ty` value: `base.x`, or `base[0]` for tuples.
    fn project(&self, base: Expr, ty: Ty<'tcx>, i: usize) -> Expr {
        match (self.shape(ty), &base.kind) {
            // A part of `[a, b]` (a `match (a, b)` subject) is just `a`.
            (Shape::Array(_), js::ExprKind::Array(items)) if !base.has_effects() => items[i].clone(),
            (Shape::Array(_), _) => Expr::index(base, Expr::int(i as i128)),
            (Shape::Object(fields), _) => Expr::member(base, fields[i].0.clone()),
            (Shape::Other, _) => unreachable!("fields of a type without fields"),
        }
    }

    /// `e` as a place, a variable and some of its fields, without reading it.
    /// Also says whether that variable is mutable.
    fn place(&self, e: ExprId) -> Option<(Expr, bool)> {
        // Inside a closure, a place it captured by value is its snapshot.
        if !self.captures.is_empty()
            && let Some(var) = self.place_path(e).and_then(|path| self.captures.get(&path))
        {
            return Some((var.place.clone(), var.mutable));
        }
        match self.thir[self.strip(e)].kind {
            ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } => {
                let var = &self.vars[&id];
                Some((var.place.clone(), var.mutable))
            }
            ExprKind::Field { lhs, name, .. } => {
                let (base, mutable) = self.place(lhs)?;
                Some((self.project(base, self.thir[lhs].ty, name.as_usize()), mutable))
            }
            // A reference is the value it points to, so `*r` is where `r` is.
            // (A static is reached through a pointer to it.)
            ExprKind::Deref { arg } if matches!(self.thir[arg].ty.kind(), ty::Ref(..) | ty::RawPtr(..)) || self.thir[arg].ty.is_box() => {
                self.place(arg)
            }
            // A JS global (ADR 0021).
            ExprKind::StaticRef { def_id, .. } if self.tcx.is_foreign_item(def_id) => {
                Some((global(&js_name(self.tcx, def_id)), false))
            }
            _ => None,
        }
    }

    /// `e` as a place whose value can't change while it's still in scope, so
    /// a pattern's variables can just name parts of it.
    ///
    /// Its variable must be immutable. That's not enough on its own: `let mut
    /// s = r;` moves `r`, and then `s.origin.x = 0` changes the object `r`
    /// still names. So the variable must also be `Copy` (read, never moved:
    /// the read copies it if needed) or hold nothing changed in place.
    fn stable_place(&self, e: ExprId) -> Option<Expr> {
        let (place, mutable) = self.place(e)?;
        let mut root = self.strip(e);
        while let ExprKind::Field { lhs, .. } | ExprKind::Deref { arg: lhs } = self.thir[root].kind {
            root = self.strip(lhs);
        }
        let ty = self.thir[root].ty;
        let unchanging = self.is_copy(ty) || !self.contains_mutated(ty);
        (!mutable && unchanging).then_some(place)
    }

    fn is_copy(&self, ty: Ty<'tcx>) -> bool {
        self.tcx.type_is_copy_modulo_regions(self.typing_env, ty)
    }

    /// The place an assignment writes to.
    fn assignee(&self, e: ExprId) -> R<Expr> {
        self.place(e).map(|(place, _)| place).ok_or_else(|| self.unsupported(self.thir[e].span, "assigning to this place"))
    }

    /// Read a variable or field's value.
    fn read(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        let ty = self.thir[e].ty;
        if let Some((place, _)) = self.place(e) {
            return Ok(self.copy_if_needed(place, ty));
        }
        match self.thir[self.strip(e)].kind {
            // A field of a temporary, like `f().x`: nothing else can see the rest.
            ExprKind::Field { lhs, name, .. } => {
                let base = self.expr(lhs, out)?;
                Ok(self.project(base, self.thir[lhs].ty, name.as_usize()))
            }
            // `*f()`, including `Deref::deref` on a `String` or `Rc`: a
            // reference is its value.
            ExprKind::Deref { arg } => self.expr(arg, out),
            _ => Err(self.unsupported(self.thir[e].span, "reading this")),
        }
    }

    /// Rust copies a `Copy` value when it's read, and JS objects are shared
    /// references. The two only disagree if one of the copies is later
    /// changed in place, which needs a type in `mutated`. So only those
    /// types are copied, and everything else stays shared.
    fn copy_if_needed(&self, place: Expr, ty: Ty<'tcx>) -> Expr {
        if self.contains_mutated(ty) && self.is_copy(ty) {
            self.copy(place, ty)
        } else {
            place
        }
    }

    fn contains_mutated(&self, ty: Ty<'tcx>) -> bool {
        self.mutated.contains(&ty)
            || match self.shape(ty) {
                Shape::Object(fields) => fields.iter().any(|&(_, t)| self.contains_mutated(t)),
                Shape::Array(tys) => tys.iter().any(|&t| self.contains_mutated(t)),
                Shape::Other => false,
            }
    }

    /// A fresh `ty` value equal to the one at `place`: `{ ...p }`, `[t[0], t[1]]`.
    /// A field that also contains mutated types is copied in turn.
    fn copy(&self, place: Expr, ty: Ty<'tcx>) -> Expr {
        match self.shape(ty) {
            Shape::Object(fields) => {
                let mut props = vec![Prop::Spread(place.clone())];
                for (name, t) in fields {
                    if self.contains_mutated(t) {
                        let field = self.copy(Expr::member(place.clone(), name.clone()), t);
                        props.push(Prop::Field(name, field));
                    }
                }
                Expr::object(props)
            }
            Shape::Array(tys) => Expr::array(
                tys.into_iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let item = Expr::index(place.clone(), Expr::int(i as i128));
                        if self.contains_mutated(t) { self.copy(item, t) } else { item }
                    })
                    .collect(),
            ),
            Shape::Other => place,
        }
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    /// A rustc span as byte offsets into the root file, for the source map.
    ///
    /// rustc numbers bytes across *all* loaded files, so subtract the root
    /// file's start. Code from a macro or desugaring maps to where it was
    /// written (`source_callsite`). Anything outside the root file (say, a
    /// `std` macro) gets no mapping.
    fn js_span(&self, span: Span) -> js::Span {
        let span = span.source_callsite();
        if span.lo() < self.file_start || span.hi() > self.file_end {
            return js::Span::NONE;
        }
        js::Span { lo: (span.lo() - self.file_start).0, hi: (span.hi() - self.file_start).0 }
    }

    fn strip(&self, e: ExprId) -> ExprId {
        strip(self.thir, e)
    }

    fn loop_index(&self, label: region::Scope, span: Span) -> R<usize> {
        self.loops
            .iter()
            .rposition(|l| l.scope == label)
            .ok_or_else(|| self.unsupported(span, "breaking out of a labeled block"))
    }

    /// JS needs a label only when jumping past the innermost loop.
    fn jump_label(&mut self, i: usize) -> Option<String> {
        if i == self.loops.len() - 1 {
            return None;
        }
        if self.loops[i].label.is_none() {
            let base = self.loops[i].label_base.clone();
            let label = fresh_in(&mut self.labels, &base);
            self.loops[i].label = Some(label);
        }
        self.loops[i].label.clone()
    }

    fn fresh(&mut self, base: &str) -> String {
        fresh_in(&mut self.names, base)
    }

    fn bind(&mut self, var: LocalVarId, name: &str, mutable: bool) -> String {
        let name = self.fresh(name);
        self.vars.insert(var, Var { place: Expr::var(&name), mutable, depth: self.loops.len() });
        name
    }

    fn num(&self, ty: Ty<'tcx>, span: Span) -> R<Num> {
        Num::of(ty).ok_or_else(|| self.unsupported(span, &format!("values of type `{ty}`")))
    }

    fn check_value_ty(&self, ty: Ty<'tcx>, span: Span) -> R<()> {
        match self.unsupported_part(ty) {
            None => Ok(()),
            Some(part) => Err(self.unsupported(span, &format!("values of type `{part}`"))),
        }
    }

    /// The first type inside `ty` (or `ty` itself) that rust-js can't represent.
    fn unsupported_part(&self, ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
        if ty.is_bool() || ty.is_unit() || ty.is_str() || Num::of(ty).is_some() {
            return None;
        }
        match ty.kind() {
            // A JS value from an `extern` block, and closures: JS functions.
            ty::Foreign(_) | ty::Closure(..) => return None,
            ty::Adt(..) if self.is_js_object(ty) => return None,
            ty::Dynamic(traits, ..)
                if traits.principal_def_id().is_some_and(|t| self.tcx.fn_trait_kind_from_def_id(t).is_some()) =>
            {
                return None;
            }
            ty::Ref(_, inner, Mutability::Not) => return self.unsupported_part(*inner),
            ty::Adt(_, _) if self.is_lang_adt(ty, LangItem::String) => return None,
            ty::Adt(_, args) if self.is_std_wrapper(ty) => return self.unsupported_part(args.type_at(0)),
            _ => {}
        }
        match (ty.kind(), self.shape(ty)) {
            (ty::Adt(adt, _), _) if is_fieldless_enum(*adt) => None,
            (_, Shape::Object(fields)) => fields.iter().find_map(|&(_, t)| self.unsupported_part(t)),
            (_, Shape::Array(tys)) => tys.iter().find_map(|&t| self.unsupported_part(t)),
            (ty::Adt(adt, _), Shape::Other) if adt.is_struct() => None, // a unit struct
            _ => Some(ty),
        }
    }

    fn check_by_value(&self, mode: BindingMode, span: Span) -> R<()> {
        if mode.0 == ByRef::No { Ok(()) } else { Err(self.unsupported(span, "`ref` bindings")) }
    }

    fn unsupported(&self, span: Span, what: &str) -> ErrorGuaranteed {
        self.tcx.dcx().span_err(span, format!("rust-js does not support {what} yet"))
    }
}

/// A JS global, like `document`, or a path from one, like `console.log`.
fn global(name: &str) -> Expr {
    let mut parts = name.split('.');
    let first = Expr::var(parts.next().unwrap_or_default());
    parts.fold(first, Expr::member)
}

/// Skip THIR's wrapper nodes that don't change meaning.
fn strip(thir: &Thir<'_>, mut e: ExprId) -> ExprId {
    loop {
        match thir[e].kind {
            ExprKind::Scope { value: inner, .. }
            | ExprKind::Use { source: inner }
            | ExprKind::NeverToAny { source: inner }
            | ExprKind::ValueTypeAscription { source: inner, .. }
            | ExprKind::PlaceTypeAscription { source: inner, .. } => e = inner,
            _ => return e,
        }
    }
}

fn is_fieldless_enum(adt: ty::AdtDef<'_>) -> bool {
    adt.is_enum() && adt.variants().iter().all(|v| v.fields.is_empty())
}

/// Turn raw constant bits into a JS number literal.
fn num_literal(bits: u128, num: Num) -> Expr {
    if num == Num::F64 {
        return Expr::num(f64::from_bits(bits as u64));
    }
    let unused = 128 - num.bits();
    let n = if num.signed() { ((bits << unused) as i128) >> unused } else { bits as i128 };
    Expr::int(n)
}

fn mask_shift(r: Expr, num: Num) -> Expr {
    Expr::bin(Op::BitAnd, r, Expr::num(num.bits() - 1))
}

fn assign_op(op: AssignOp) -> BinOp {
    match op {
        AssignOp::AddAssign => BinOp::Add,
        AssignOp::SubAssign => BinOp::Sub,
        AssignOp::MulAssign => BinOp::Mul,
        AssignOp::DivAssign => BinOp::Div,
        AssignOp::RemAssign => BinOp::Rem,
        AssignOp::BitXorAssign => BinOp::BitXor,
        AssignOp::BitAndAssign => BinOp::BitAnd,
        AssignOp::BitOrAssign => BinOp::BitOr,
        AssignOp::ShlAssign => BinOp::Shl,
        AssignOp::ShrAssign => BinOp::Shr,
    }
}

/// Pick an unused name: `x`, then `x$1`, `x$2`, ... Rust identifiers can't
/// contain `$`, so these never clash with a user's name.
fn fresh_in(taken: &mut HashSet<String>, base: &str) -> String {
    let base = js_ident(base);
    if taken.insert(base.clone()) {
        return base;
    }
    (1..).map(|k| format!("{base}${k}")).find(|name| taken.insert(name.clone())).unwrap()
}

/// Rust names that mean something else in JS get a `$` suffix.
fn js_ident(name: &str) -> String {
    const RESERVED: &[&str] = &[
        "arguments", "await", "break", "case", "catch", "class", "const", "continue", "debugger",
        "default", "delete", "do", "else", "enum", "eval", "export", "extends", "false", "finally",
        "for", "function", "if", "implements", "import", "in", "instanceof", "interface", "let",
        "new", "null", "package", "private", "protected", "public", "return", "static", "super",
        "switch", "this", "throw", "true", "try", "typeof", "var", "void", "while", "with",
        "yield", "undefined", "NaN", "Infinity", "Math", "Error", "String",
    ];
    if RESERVED.contains(&name) { format!("{name}$") } else { name.to_string() }
}
