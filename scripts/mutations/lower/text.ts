// Mutations of src/lower/text.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "as-bytes-utf16",
    breaks: "`s.as_bytes()` is each UTF-16 unit, `é` one byte where UTF-8 has two",
    file: "src/lower/text.rs",
    find: "                Expr::call(Expr::member(Expr::var(\"Array\"), \"from\"), vec![encoded])\n            }",
    replace: "                let _ = encoded;\n                Expr::call(Expr::member(Expr::var(\"Array\"), \"from\"), vec![arg(), Expr::arrow(vec![\"c\".into()], vec![crate::js::StmtKind::Return(Some(Expr::call(Expr::member(Expr::var(\"c\"), \"charCodeAt\"), vec![Expr::int(0)]))).at(crate::js::Span::NONE)])])\n            }",
    tests: ["test/corpus.test.ts","-t","byte_strings"],
  },
  {
    name: "slice-to-inclusive-end-dropped",
    breaks: "`&v[..=1]` stops before index 1",
    file: "src/lower/text.rs",
    find: "                    (RangeKind::ToInclusive, [end]) => (Expr::int(0), Some(past(end))),",
    replace: "                    (RangeKind::ToInclusive, [end]) => (Expr::int(0), Some(end.clone())),",
    tests: ["test/corpus.test.ts","-t","range_values"],
  },
  {
    name: "str-full-slice-checked",
    breaks: "`&s[..]` of a string is checked by `$strSlice`: right, but not the JS a person writes",
    file: "src/lower/text.rs",
    find: "        if op == TextOp::StrSlice && start.is_none() && end.is_none() {",
    replace: "        if false {",
    tests: ["test/corpus.test.ts", "-t", "string_bytes"],
    snapshots: true,
  },
];
