// Mutations of src/lower/numbers.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "f32-arithmetic-unrounded",
    breaks: "an `f32`'s `+` is a double's, which may be no `f32` at all: ten tenths are 1.0000000149011612",
    file: "src/lower/numbers.rs",
    find: "            return Ok(if num == Num::F32 && op != BinOp::Rem {\n                num.wrap(e)\n",
    replace: "            return Ok(if false {\n                num.wrap(e)\n",
    tests: ["test/corpus.test.ts", "-t", "f32\\.rs"],
  },
  {
    name: "big-to-f32-through-f64",
    breaks: "`n as f32` of a `u64` is rounded to an `f64` first, and then can't be told from a tie",
    file: "src/lower/numbers.rs",
    find: '                Ok(Expr::call(Expr::var("$bigToF32"), vec![v]))',
    replace: '                Ok(target.wrap(Expr::call(Expr::var("Number"), vec![v])))',
    tests: ["test/corpus.test.ts", "-t", "f32\\.rs"],
  },
  {
    name: "powi-f32-rounded-once",
    breaks: "`x.powi(n)` of an `f32` multiplies as doubles, where compiler-rt rounds each product",
    file: "src/lower/numbers.rs",
    find: "            NumOp::Powi if num == Num::F32 =>",
    replace: "            NumOp::Powi if false && num == Num::F32 =>",
    tests: ["test/corpus.test.ts", "-t", "f32\\.rs"],
  },
  {
    name: "exp2-is-exp",
    breaks: "`x.exp2()` is `e ** x`",
    file: "src/lower/numbers.rs",
    find: "            NumOp::Exp2 => rounded(Expr::bin(Op::Pow, Expr::num(2.0), arg())),",
    replace: "            NumOp::Exp2 => rounded(Expr::bin(Op::Pow, Expr::num(std::f64::consts::E), arg())),",
    tests: ["test/corpus.test.ts","-t","std_methods"],
  },
  {
    name: "size-align-swap",
    breaks: "`align_of` is the type's size",
    file: "src/lower/numbers.rs",
    find: "        let bytes = if matches!(known, Std::AlignOf) {\n            layout.align.abi.bytes()",
    replace: "        let bytes = if matches!(known, Std::AlignOf) {\n            layout.size.bytes()",
    tests: ["test/corpus.test.ts", "-t", "size_of\\.rs"],
  },
  {
    name: "layout-in-function-environment",
    breaks: "`size_of_val` of a generic `async fn`'s future is laid out in the function's environment, where 1.98 finds it too generic, and is refused",
    file: "src/lower/numbers.rs",
    find: "            .layout_of(ty::TypingEnv::fully_monomorphized().as_query_input(of))\n",
    replace: "            .layout_of(self.typing_env.as_query_input(of))\n",
    tests: ["test/corpus.test.ts","-t","future_sizes"],
  },
];
