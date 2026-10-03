// Mutations of src/lower/analysis/validation.rs (ADR 0093).
import type { Mutation } from "../../../mutations";

export const mutations: Mutation[] = [
  {
    name: "static-mut-reference",
    breaks: "a `&mut` to a `static mut` is allowed",
    file: "src/lower/analysis/validation.rs",
    find: '                Some(d) if tcx.is_mutable_static(d) => "`&mut` references to a `static mut`",',
    replace: '                Some(d) if tcx.is_mutable_static(d) && false => "`&mut` references to a `static mut`",',
    tests: ["test/corpus.test.ts", "-t", "static_mut_reference"],
  },
  {
    name: "static-mut-shared-reference",
    breaks: "a shared reference to a `static mut` is rejected",
    file: "src/lower/analysis/validation.rs",
    find: "                ExprKind::Borrow {\n                    borrow_kind: BorrowKind::Mut { .. },\n                    arg,\n                } => (arg, false),\n",
    replace: "                ExprKind::Borrow { arg, .. } => (arg, false),\n",
    tests: ["test/corpus.test.ts", "-t", "static_mut_shared"],
  },
  {
    name: "extern-of-own-export",
    breaks: "an `extern` declaration of the crate's own `#[no_mangle]` function is a JS global no JS has",
    file: "src/lower/analysis/validation.rs",
    find: "                if tcx.is_foreign_item(def_id)\n",
    replace: "                if false && tcx.is_foreign_item(def_id)\n",
    tests: ["test/diagnostics.test.ts","-t","no_mangle"],
  },
  {
    name: "generic-consts-accepted",
    breaks: "a constant with parameters of its own, `const SIZE<T>`, is accepted, which a dictionary has no place for",
    file: "src/lower/analysis/validation.rs",
    find: "            DefKind::AssocConst { .. } if tcx.generics_of(def_id).own_params.is_empty() => continue,\n",
    replace: "            DefKind::AssocConst { .. } if true => continue,\n",
    tests: ["test/diagnostics.test.ts","-t","a generic constant"],
  },
  {
    name: "type-consts-accepted",
    breaks: "a `type const` is accepted, and rustc crashes type-checking the body it hasn't",
    file: "src/lower/analysis/validation.rs",
    find: "            DefKind::AssocConst { is_type_const: true } => \"type constants\",\n",
    replace: "",
    tests: ["test/diagnostics.test.ts","-t","a type constant"],
  },
  {
    name: "externally-implementable-accepted",
    breaks: "`#[eii] static HELLO: u64;` is accepted, and its JS reads a `HELLO` nothing defines",
    file: "src/lower/analysis/validation.rs",
    find: "            _ if find_attr!(tcx, def_id, EiiImpls(..) | EiiDeclaration(..) | RustcEiiForeignItem) => {\n",
    replace: "            _ if false => {\n",
    tests: ["test/diagnostics.test.ts","-t","externally implementable"],
  },
  {
    name: "gats-refused",
    breaks: "a generic associated type of lifetimes, `type Iter<'a>`, is an error",
    file: "src/lower/analysis/validation.rs",
    find: "            DefKind::AssocTy if traits::gat_supported(tcx, def_id.to_def_id()) => continue,",
    replace: "            DefKind::AssocTy if false && traits::gat_supported(tcx, def_id.to_def_id()) => continue,",
    tests: ["test/corpus.test.ts", "-t", "generic_associated_types"],
  },
];
