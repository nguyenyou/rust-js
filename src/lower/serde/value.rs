//! serde_json's `Value` (ADR 0083): an enum as rust-js makes one, `"Null"`
//! or `{ TAG: "String", _0: s }`, so a `match` on one is a `match` on any
//! enum. Its `Map<String, Value>` is a JS `Map`, sorted as the `BTreeMap`
//! it wraps is, and its `Number` is `{ kind, value }`, a `"u"`nsigned,
//! `"i"`nteger or `"f"`loat, as serde_json keeps one.

use crate::js::{self, Expr, Op, Prop, Stmt, StmtKind};
use crate::lower::recognition::{Json, JsonCall, JsonMethod};
use crate::lower::representation::Num;
use crate::lower::{FnCx, R};
use crate::runtime::Helper;
use rustc_middle::thir::ExprId;
use rustc_middle::ty::{self, Ty};
use rustc_span::def_id::DefId;
use rustc_span::{Span, sym};

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// `serde_json::Value`, `Number` or `Map`.
    pub(in crate::lower) fn json_type(&self, ty: Ty<'tcx>) -> Option<Json> {
        self.recognition().json_type(ty)
    }

    pub(in crate::lower) fn is_json_map(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_json_map(ty)
    }

    pub(in crate::lower) fn is_json_number(&self, ty: Ty<'tcx>) -> bool {
        self.json_type(ty) == Some(Json::Number)
    }

    /// A call of serde_json's that rust-js writes itself: `to_value`, a
    /// `Value`'s or a `Number`'s method, `value[key]`, `==` of a `Value`
    /// and a string, a number or a `bool`, `Value::from`, and `into()` one.
    pub(in crate::lower) fn json_call(
        &mut self,
        def_id: DefId,
        generic_args: ty::GenericArgsRef<'tcx>,
        args: &[ExprId],
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Option<Expr>> {
        let Some(operation) = self.recognition().json_call(def_id, generic_args) else {
            return Ok(None);
        };
        match operation {
            JsonCall::ToValue(ty) => {
                let value = self.expr(args[0], out)?;
                self.use_value();
                // Of a string, a number, a `bool` or `()`, it can't fail: `Ok` of it.
                if self.is_string_like(ty) || ty.is_bool() || ty.is_unit() || Num::of(ty).is_some() {
                    let value = if ty.is_char() {
                        Expr::object(vec![
                            Prop::Field("TAG".into(), Expr::str("String")),
                            Prop::Field("_0".into(), value),
                        ])
                    } else {
                        self.json_value_from(value, ty, span)?
                    };
                    return Ok(Some(Expr::object(vec![
                        Prop::Field("TAG".into(), Expr::str("Ok")),
                        Prop::Field("_0".into(), value),
                    ])));
                }
                let writer = self.json_writer(ty, span)?;
                Ok(Some(Expr::call(Expr::var("$toJsonValue"), vec![value, writer])))
            }
            JsonCall::FromValue(ty) => {
                let value = self.expr(args[0], out)?;
                let reader = self.json_reader(ty, span)?;
                self.use_value();
                self.runtime.insert(Helper::FromJson);
                Ok(Some(Expr::call(Expr::var("$fromJsonValue"), vec![value, reader])))
            }
            JsonCall::Index => {
                let values = self.operands(args, out)?;
                self.use_value();
                Ok(Some(Expr::call(Expr::var("$jsonIndex"), values)))
            }
            JsonCall::Equal {
                other,
                value_first,
                negate,
            } => {
                let mut values = self.operands(args, out)?;
                let (b, a) = (values.pop().expect("two sides"), values.pop().expect("two sides"));
                let eq = if value_first {
                    self.json_value_eq(a, b, other, span)?
                } else {
                    self.json_value_eq(b, a, other, span)?
                };
                Ok(Some(if negate {
                    crate::lower::std_impls::negate(eq)
                } else {
                    eq
                }))
            }
            JsonCall::Convert { to, from, json } => {
                let x = self.expr(args[0], out)?;
                Ok(Some(match json {
                    Json::Value => self.json_value_from(x, from, span)?,
                    Json::Number if Num::of(from.peel_refs()).is_some_and(|n| n != Num::F64) => {
                        self.use_value();
                        Expr::call(Expr::var("$jsonInt"), vec![x])
                    }
                    _ => return Err(self.unsupported(span, &format!("`{to}::from` of `{from}`"))),
                }))
            }
            JsonCall::Default => Ok(Some(Expr::str("Null"))),
            JsonCall::Method(method) => {
                let values = self.operands(args, out)?;
                self.json_value_method(method, values, span).map(Some)
            }
        }
    }

    fn use_value(&mut self) {
        self.runtime.insert(Helper::JsonValue);
    }

    /// `$jsonValueWrite` and `$jsonNumberWrite`: what writes a `Value` or a
    /// `Number`, `(value, json) => ..`.
    pub(in crate::lower) fn json_value_writer(&mut self, ty: Ty<'tcx>) -> Option<Expr> {
        let name = match self.json_type(ty)? {
            Json::Value => "$jsonValueWrite",
            Json::Number => "$jsonNumberWrite",
            Json::Map => return None,
        };
        self.use_value();
        Some(Expr::var(name))
    }

    /// `clone()` of a `Value` or a `Map`: a copy of its arrays and maps.
    pub(in crate::lower) fn json_value_clone(&mut self, value: Expr, ty: Ty<'tcx>) -> Option<Expr> {
        let name = match self.json_type(ty)? {
            Json::Value => "$jsonValueClone",
            Json::Map => "$jsonMapClone",
            Json::Number => return None,
        };
        self.use_value();
        Some(Expr::call(Expr::var(name), vec![value]))
    }

    /// `{}` of a `Value` (its compact JSON, or with `{:#}`, its pretty) or a
    /// `Number`.
    pub(in crate::lower) fn json_value_display(&mut self, value: Expr, ty: Ty<'tcx>, pretty: bool) -> Option<Expr> {
        let json = self.json_type(ty)?;
        self.use_value();
        Some(match json {
            Json::Value => Expr::call(Expr::var("$jsonValueText"), vec![value, Expr::bool(pretty)]),
            Json::Number => Expr::call(Expr::var("$jsonNumberText"), vec![value]),
            Json::Map => return None,
        })
    }

    /// `{:?}` of a `Value` or a `Number`, as serde_json writes one:
    /// `Object {"a": Number(1)}`.
    pub(in crate::lower) fn json_value_debug(&mut self, value: Expr, ty: Ty<'tcx>) -> Option<Expr> {
        let json = self.json_type(ty)?;
        self.use_value();
        Some(match json {
            Json::Value => Expr::call(Expr::var("$debugJsonValue"), vec![value]),
            Json::Number => Expr::call(Expr::var("$debugJsonNumber"), vec![value]),
            Json::Map => return None,
        })
    }

    /// A method of `Value` or `Number`, as serde_json's does it.
    fn json_value_method(&mut self, method: JsonMethod, mut values: Vec<Expr>, span: Span) -> R<Expr> {
        self.use_value();
        // `Number::from_f64(x)`: `None` unless it's finite.
        match method {
            JsonMethod::NumberFromF64 => return Ok(Expr::call(Expr::var("$jsonNumberOfF64"), values)),
            JsonMethod::Unsupported { owner, name } => {
                return Err(self.unsupported(span, &format!("`{owner}::{name}`")));
            }
            _ => {}
        }
        let receiver = values.remove(0);
        let call = |helper: &str, mut args: Vec<Expr>| {
            args.insert(0, receiver.clone());
            Expr::call(Expr::var(helper), args)
        };
        Ok(match method {
            JsonMethod::NumberAsF64 => call("$jsonNumberF64", Vec::new()),
            JsonMethod::NumberKind(kind) => Expr::bin(Op::Eq, Expr::member(receiver.clone(), "kind"), Expr::str(kind)),
            JsonMethod::NumberIsI64 => call("$jsonNumberIsI64", Vec::new()),
            JsonMethod::IsNull => Expr::bin(Op::Eq, receiver.clone(), Expr::str("Null")),
            JsonMethod::IsTag(tag) => Expr::bin(Op::Eq, Expr::member(receiver.clone(), "TAG"), Expr::str(tag)),
            JsonMethod::IsNumber(kind) => call("$jsonValueIs", vec![Expr::str(kind)]),
            JsonMethod::AsTag(tag) => call("$jsonValueAs", vec![Expr::str(tag)]),
            JsonMethod::AsF64 => call("$jsonValueF64", Vec::new()),
            // `get(k)` of an object, or `get(i)` of an array: `None` if it
            // isn't there.
            JsonMethod::Get => {
                let key = values.remove(0);
                call("$jsonGet", vec![key])
            }
            JsonMethod::NumberFromF64 | JsonMethod::Unsupported { .. } => {
                unreachable!("handled before receiver extraction")
            }
        })
    }

    /// `Value::from(x)` and `x.into()`: `x`, as serde_json's `From` impls
    /// make a `Value` of it.
    pub(in crate::lower) fn json_value_from(&mut self, value: Expr, from: Ty<'tcx>, span: Span) -> R<Expr> {
        self.use_value();
        let from = from.peel_refs();
        let variant = |tag: &str, value: Expr| {
            Expr::object(vec![
                Prop::Field("TAG".into(), Expr::str(tag)),
                Prop::Field("_0".into(), value),
            ])
        };
        if self.is_string_like(from) && !from.is_char() {
            return Ok(variant("String", value));
        }
        if from.is_bool() {
            return Ok(variant("Bool", value));
        }
        if from.is_unit() {
            return Ok(Expr::str("Null"));
        }
        match Num::of(from) {
            Some(Num::F64) => return Ok(Expr::call(Expr::var("$jsonFloat"), vec![value])),
            Some(_) => return Ok(variant("Number", Expr::call(Expr::var("$jsonInt"), vec![value]))),
            None => {}
        }
        match self.json_type(from) {
            Some(Json::Map) => return Ok(variant("Object", value)),
            Some(Json::Number) => return Ok(variant("Number", value)),
            Some(Json::Value) => return Ok(value),
            None => {}
        }
        // A `Vec` of what makes one, or an `Option` of one: `Null` for `None`.
        let convert = |this: &mut Self, item: Ty<'tcx>| -> R<Expr> {
            let x = this.fresh("x");
            let converted = this.json_value_from(Expr::var(&x), item, span)?;
            Ok(Expr::arrow(
                vec![x.into()],
                vec![StmtKind::Return(Some(converted)).at(js::Span::NONE)],
            ))
        };
        if let ty::Adt(_, args) = from.kind()
            && self.is_std_adt(from, sym::Vec)
        {
            let arrow = convert(self, args.type_at(0))?;
            return Ok(variant("Array", Expr::call(Expr::member(value, "map"), vec![arrow])));
        }
        if let Some(inner) = self.option_of(from) {
            let arrow = convert(self, inner)?;
            return Ok(Expr::call(Expr::var("$jsonOption"), vec![value, arrow]));
        }
        Err(self.unsupported(span, &format!("`Value::from` of `{from}`")))
    }

    /// `value == x` of a `Value` and a string, a number or a `bool`, as
    /// serde_json's `PartialEq` impls compare them.
    pub(in crate::lower) fn json_value_eq(
        &mut self,
        value: Expr,
        other: Expr,
        other_ty: Ty<'tcx>,
        span: Span,
    ) -> R<Expr> {
        self.use_value();
        let other_ty = other_ty.peel_refs();
        let kind = if self.is_string_like(other_ty) {
            "String"
        } else if other_ty.is_bool() {
            "Bool"
        } else {
            match Num::of(other_ty) {
                Some(Num::F64) => "f64",
                Some(n) if n.signed() => "i64",
                Some(_) => "u64",
                None => return Err(self.unsupported(span, &format!("`==` of a `Value` and a `{other_ty}`"))),
            }
        };
        Ok(Expr::call(
            Expr::var("$jsonValueEq"),
            vec![value, other, Expr::str(kind)],
        ))
    }
}
