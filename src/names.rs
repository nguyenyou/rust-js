//! JavaScript identifier policy shared by lowering and linking.

use std::collections::HashSet;

/// Pick an unused name: `x`, then `x$1`, `x$2`, ... Rust identifiers can't
/// contain `$`, so these never clash with a user's name.
pub(crate) fn fresh_in(taken: &mut HashSet<String>, base: &str) -> String {
    let base = js_ident(base);
    if taken.insert(base.clone()) {
        return base;
    }
    (1..)
        .map(|k| format!("{base}${k}"))
        .find(|name| taken.insert(name.clone()))
        .unwrap()
}

/// Rust names that mean something else in JS get a `$` suffix.
pub(crate) fn js_ident(name: &str) -> String {
    const RESERVED: &[&str] = &[
        "arguments",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "eval",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "function",
        "if",
        "implements",
        "import",
        "in",
        "instanceof",
        "interface",
        "let",
        "new",
        "null",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        "yield",
        "undefined",
        "NaN",
        "Infinity",
        "Math",
        "Error",
        "String",
        "WeakMap",
        "DataView",
        "ArrayBuffer",
        "Number",
        "BigInt",
        "Object",
    ];
    if RESERVED.contains(&name) {
        format!("{name}$")
    } else {
        name.to_string()
    }
}
