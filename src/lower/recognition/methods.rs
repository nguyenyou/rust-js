//! Pure method-name tables, called only after library/type identity checks.

use super::{Comb, IterComb, Num, NumOp, TextOp};
use rustc_middle::mir::BinOp;

/// Which `TextOp` a method of a `char` or a `str` is.
pub(super) fn text(name: &str, char: bool, str: bool) -> Option<TextOp> {
    Some(match name {
        "is_whitespace" if char => TextOp::Is("/^\\p{White_Space}$/u"),
        "is_alphabetic" if char => TextOp::Is("/^\\p{Alphabetic}$/u"),
        "is_numeric" if char => TextOp::Is("/^\\p{N}$/u"),
        "is_alphanumeric" if char => TextOp::Is("/^[\\p{Alphabetic}\\p{N}]$/u"),
        "is_uppercase" if char => TextOp::Is("/^\\p{Uppercase}$/u"),
        "is_lowercase" if char => TextOp::Is("/^\\p{Lowercase}$/u"),
        "is_control" if char => TextOp::Is("/^\\p{Cc}$/u"),
        "is_ascii_digit" if char => TextOp::Is("/^[0-9]$/"),
        "is_ascii_hexdigit" if char => TextOp::Is("/^[0-9A-Fa-f]$/"),
        "is_ascii_alphabetic" if char => TextOp::Is("/^[A-Za-z]$/"),
        "is_ascii_alphanumeric" if char => TextOp::Is("/^[A-Za-z0-9]$/"),
        "is_ascii_uppercase" if char => TextOp::Is("/^[A-Z]$/"),
        "is_ascii_lowercase" if char => TextOp::Is("/^[a-z]$/"),
        "is_ascii_whitespace" if char => TextOp::Is("/^[ \\t\\n\\f\\r]$/"),
        "is_ascii_punctuation" if char => TextOp::Is("/^[!-\\/:-@[-`{-~]$/"),
        "is_ascii_graphic" if char => TextOp::Is("/^[!-~]$/"),
        "is_ascii_control" if char => TextOp::Is("/^[\\0-\\x1f\\x7f]$/"),
        "is_ascii" if char => TextOp::IsAscii,
        "to_ascii_uppercase" if char => TextOp::ToAsciiUpper,
        "to_ascii_lowercase" if char => TextOp::ToAsciiLower,
        "to_digit" if char => TextOp::ToDigit,
        "to_uppercase" if char => TextOp::CharCase(true),
        "to_lowercase" if char => TextOp::CharCase(false),
        "is_digit" if char => TextOp::IsDigit,
        "split_whitespace" if str => TextOp::SplitWhitespace,
        "lines" if str => TextOp::Lines,
        "as_bytes" if str => TextOp::Bytes,
        "parse" if str => TextOp::Parse,
        _ => return None,
    })
}

/// Which `NumOp` a method of a number is.
pub(super) fn number(name: &str, num: Num) -> Option<NumOp> {
    let float = num.float();
    let signed = num.signed();
    Some(match name {
        "floor" | "ceil" | "trunc" | "sqrt" | "cbrt" | "exp" | "log10" | "log2" | "sin" | "cos" | "tan" | "asin"
        | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "hypot" | "atan2"
            if float =>
        {
            NumOp::Math(match name {
                "floor" => "floor",
                "ceil" => "ceil",
                "trunc" => "trunc",
                "sqrt" => "sqrt",
                "cbrt" => "cbrt",
                "exp" => "exp",
                "log10" => "log10",
                "log2" => "log2",
                "sin" => "sin",
                "cos" => "cos",
                "tan" => "tan",
                "asin" => "asin",
                "acos" => "acos",
                "atan" => "atan",
                "sinh" => "sinh",
                "cosh" => "cosh",
                "tanh" => "tanh",
                "hypot" => "hypot",
                _ => "atan2",
            })
        }
        "ln" if float => NumOp::Math("log"),
        "exp_m1" if float => NumOp::Math("expm1"),
        "ln_1p" if float => NumOp::Math("log1p"),
        "exp2" if float => NumOp::Exp2,
        "abs" if float => NumOp::Math("abs"),
        "abs" if signed => NumOp::Abs,
        "unsigned_abs" if signed => NumOp::UnsignedAbs,
        "pow" if !float => NumOp::Pow,
        "checked_pow" if !float => NumOp::CheckedPow,
        "powi" if float => NumOp::Powi,
        "powf" if float => NumOp::Powf,
        "round" if float => NumOp::Round,
        "total_cmp" if float => NumOp::TotalCmp,
        "is_nan" if float => NumOp::IsNan,
        "is_finite" if float => NumOp::IsFinite,
        "is_infinite" if float => NumOp::IsInfinite,
        "checked_add" if !float => NumOp::Checked(BinOp::Add),
        "checked_sub" if !float => NumOp::Checked(BinOp::Sub),
        "checked_mul" if !float => NumOp::Checked(BinOp::Mul),
        "checked_div" if !float => NumOp::Checked(BinOp::Div),
        "saturating_add" if !float => NumOp::Saturating(BinOp::Add),
        "saturating_sub" if !float => NumOp::Saturating(BinOp::Sub),
        "saturating_mul" if !float => NumOp::Saturating(BinOp::Mul),
        "wrapping_add" if !float => NumOp::Wrapping(BinOp::Add),
        "wrapping_sub" if !float => NumOp::Wrapping(BinOp::Sub),
        "wrapping_mul" if !float => NumOp::Wrapping(BinOp::Mul),
        "rem_euclid" if !float => NumOp::RemEuclid,
        "div_euclid" if !float => NumOp::DivEuclid,
        "signum" if signed => NumOp::Signum,
        "leading_zeros" if !float => NumOp::LeadingZeros,
        "trailing_zeros" if !float => NumOp::TrailingZeros,
        "count_ones" if !float => NumOp::CountOnes,
        "is_power_of_two" if !float && !signed => NumOp::IsPowerOfTwo,
        "abs_diff" if !float => NumOp::AbsDiff,
        _ => return None,
    })
}

/// Which `Comb` a method of an `Option`, `Result` or `Vec` is.
pub(super) fn combinator(name: &str, option: bool, result: bool, vec: bool, slice: bool) -> Option<Comb> {
    Some(match name {
        "unwrap_or_else" if option => Comb::UnwrapOrElse,
        "unwrap_or_default" if option => Comb::UnwrapOrDefault,
        "map_or" if option => Comb::MapOr,
        "map_or_else" if option => Comb::MapOrElse,
        "and_then" if option => Comb::AndThen,
        "filter" if option => Comb::Filter,
        "ok_or" if option => Comb::OkOr,
        "ok_or_else" if option => Comb::OkOrElse,
        "or" if option => Comb::Or,
        "or_else" if option => Comb::OrElse,
        "is_some_and" if option => Comb::IsSomeAnd,
        "is_none_or" if option => Comb::IsNoneOr,
        "map" if result => Comb::ResultMap,
        "map_err" if result => Comb::MapErr,
        "and_then" if result => Comb::ResultAndThen,
        "unwrap_or_else" if result => Comb::ResultUnwrapOrElse,
        "unwrap_or_default" if result => Comb::ResultUnwrapOrDefault,
        "err" if result => Comb::Err,
        "is_ok_and" if result => Comb::IsOkAnd,
        "is_err_and" if result => Comb::IsErrAnd,
        "contains" if slice => Comb::Contains,
        "binary_search" if slice => Comb::BinarySearch,
        "split_off" if vec => Comb::SplitOff,
        "insert" if vec => Comb::Insert,
        "remove" if vec => Comb::Remove,
        "swap" if slice => Comb::Swap,
        "truncate" if vec => Comb::Truncate,
        "dedup" if vec => Comb::Dedup,
        "windows" if slice => Comb::Windows,
        "chunks" if slice => Comb::Chunks,
        "concat" if slice => Comb::Concat,
        _ => return None,
    })
}

/// Which `Comb` a method of a `bool` is.
pub(super) fn boolean(name: &str, boolean: bool) -> Option<Comb> {
    match name {
        "then" if boolean => Some(Comb::Then),
        "then_some" if boolean => Some(Comb::ThenSome),
        _ => None,
    }
}

/// Which `IterComb` an `Iterator` method is.
pub(super) fn iterator(name: &str) -> Option<IterComb> {
    Some(match name {
        "filter_map" => IterComb::FilterMap,
        "flat_map" => IterComb::FlatMap,
        "flatten" => IterComb::Flatten,
        "zip" => IterComb::Zip,
        "chain" => IterComb::Chain,
        "take_while" => IterComb::TakeWhile,
        "skip_while" => IterComb::SkipWhile,
        "step_by" => IterComb::StepBy,
        "scan" => IterComb::Scan,
        "max_by_key" => IterComb::MaxByKey(true),
        "min_by_key" => IterComb::MaxByKey(false),
        "max_by" => IterComb::MaxBy(true),
        "min_by" => IterComb::MaxBy(false),
        "product" => IterComb::Product,
        "nth" => IterComb::Nth,
        "find_map" => IterComb::FindMap,
        "partition" => IterComb::Partition,
        "inspect" => IterComb::Inspect,
        "unzip" => IterComb::Unzip,
        _ => return None,
    })
}
