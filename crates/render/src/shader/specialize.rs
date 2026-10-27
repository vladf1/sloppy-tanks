//! Removes the code a variant's flags disable before the browser sees it.
//!
//! The templates branch on `const` flags (`if HAS_BUMP { ... }`) so one source serves
//! every variant, but neither Tint nor WebKit folds those branches: the translated
//! MSL keeps every disabled feature behind `if (false)`, and the Metal compiler
//! parses and lowers all of it before optimizing it away. On a cold shader cache that
//! made an unlit variant cost as much as a lit one (about 175 ms each in WebKit, which
//! compiles pipelines one at a time). So a variant's source keeps only the taken side
//! of each flag branch and the functions its entry points still reach, and drops the
//! comments.
//!
//! It also unrolls the counted loops (`unroll_loops`). WebKit guards every WGSL loop
//! against running forever with a `volatile` iteration counter in the MSL it
//! generates, and the Metal compiler can neither unroll such a loop nor keep what the
//! loop indexes in registers: a shadow filter's tap arrays and a noise cell search's
//! candidates went to stack memory, and the village's main view took 75-95 ms of GPU
//! time on an M3 Max in WebKit against 5 ms once its loops were unrolled (and in
//! Chrome, whose Metal compile unrolls them itself).

use std::collections::HashMap;

/// `code` with every `if FLAG { ... } [else ...]` whose condition is one of `flags`
/// (optionally negated) replaced by the taken block, then without comments, counted
/// loops, unreachable functions and the declarations of flags nothing reads any more
/// (so variants that fold to the same code are the same text for the browser's caches).
pub fn specialize(code: &str, flags: &[(&str, bool)]) -> String {
    let code = strip_comments(code);
    let code = fold_flag_branches(&code, flags);
    let code = unroll_loops(&code);
    let mut code = prune_functions(&code);
    for (name, value) in flags {
        let declaration = format!("const {name}: bool = {value};\n");
        if let Some(at) = code.find(&declaration) {
            let bytes = code.as_bytes();
            let used = code
                .match_indices(name)
                .any(|(i, _)| (i < at || i >= at + declaration.len()) && word_at(bytes, i, name));
            if !used {
                code.replace_range(at..at + declaration.len(), "");
            }
        }
    }
    code
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Removes `//` and `/* */` comments (WGSL has no string literals) and blank lines.
fn strip_comments(code: &str) -> String {
    let bytes = code.as_bytes();
    let mut out = String::with_capacity(code.len());
    let mut i = 0;
    let mut start = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
            out.push_str(&code[start..i]);
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            start = i;
        } else if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            out.push_str(&code[start..i]);
            // WGSL block comments nest.
            let mut depth = 0;
            while i < bytes.len() {
                if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            start = i;
        } else {
            i += 1;
        }
    }
    out.push_str(&code[start..]);
    let mut lines = String::with_capacity(out.len());
    for line in out
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
    {
        lines.push_str(line);
        lines.push('\n');
    }
    lines
}

/// The index just past the `}` matching the `{` at `open`.
fn block_end(bytes: &[u8], open: usize) -> usize {
    debug_assert_eq!(bytes[open], b'{');
    let mut depth = 0;
    for (offset, &byte) in bytes[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return open + offset + 1;
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces in WGSL template");
}

fn skip_space(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// The end of the identifier starting at `i`.
fn ident_end(bytes: &[u8], i: usize) -> usize {
    i + bytes[i..].iter().take_while(|&&b| is_ident(b)).count()
}

/// Whether `word` starts at `i` as a whole identifier.
pub(super) fn word_at(bytes: &[u8], i: usize, word: &str) -> bool {
    // Every byte of every variant's source is tested: reject on the first byte
    // before comparing the rest.
    word.as_bytes()
        .first()
        .is_none_or(|first| bytes.get(i) == Some(first))
        && bytes[i..].starts_with(word.as_bytes())
        && (i == 0 || !is_ident(bytes[i - 1]))
        && bytes.get(i + word.len()).is_none_or(|&b| !is_ident(b))
}

/// The end of the `if ... { } [else ...]` statement starting at `start`.
fn if_statement_end(bytes: &[u8], start: usize) -> usize {
    let open = start + bytes[start..].iter().position(|&b| b == b'{').unwrap();
    let end = block_end(bytes, open);
    let next = skip_space(bytes, end);
    if next < bytes.len() && word_at(bytes, next, "else") {
        let rest = skip_space(bytes, next + 4);
        if bytes[rest] == b'{' {
            return block_end(bytes, rest);
        }
        return if_statement_end(bytes, rest);
    }
    end
}

/// A flag condition at `i` (just past `if`): the flag's value, negation applied, and
/// the index of the block's `{`.
fn flag_condition(bytes: &[u8], i: usize, flags: &[(&str, bool)]) -> Option<(bool, usize)> {
    let mut i = skip_space(bytes, i);
    let negated = bytes.get(i) == Some(&b'!');
    if negated {
        i = skip_space(bytes, i + 1);
    }
    let name_end = ident_end(bytes, i);
    let name = std::str::from_utf8(&bytes[i..name_end]).ok()?;
    let value = flags.iter().find(|(flag, _)| *flag == name)?.1;
    let open = skip_space(bytes, name_end);
    (bytes.get(open) == Some(&b'{')).then_some((value != negated, open))
}

/// Replaces each statement `if FLAG { A } else B` with `{ A }` or `B` (a block or the
/// rest of an `else if` chain; nothing without `else`). A chain whose head is not a
/// flag is left alone.
fn fold_flag_branches(code: &str, flags: &[(&str, bool)]) -> String {
    let mut code = code.to_owned();
    let mut from = 0;
    loop {
        let bytes = code.as_bytes();
        let Some(found) = (from..bytes.len().saturating_sub(2)).find(|&i| word_at(bytes, i, "if"))
        else {
            return code;
        };
        from = found + 2;
        let before = code[..found].trim_end();
        let chained = before.len() >= 4 && word_at(before.as_bytes(), before.len() - 4, "else");
        if chained {
            continue;
        }
        let Some((taken, open)) = flag_condition(bytes, found + 2, flags) else {
            continue;
        };
        let then_end = block_end(bytes, open);
        let end = if_statement_end(bytes, found);
        let replacement = if taken {
            code[open..then_end].to_owned()
        } else {
            let next = skip_space(bytes, then_end);
            if next < end {
                // Past `else`: a block, or an `if` chain that is folded in turn.
                code[skip_space(bytes, next + 4)..end].to_owned()
            } else {
                String::new()
            }
        };
        code.replace_range(found..end, &replacement);
        from = found;
    }
}

/// The most copies a loop unrolls into; a longer loop stays a loop.
const MAX_UNROLLED_ITERATIONS: i64 = 16;

/// A `for` statement's header: its variable, its `{`, and the values the variable
/// takes when the bounds are integers known here (first, past the last, and the
/// literal suffix that gives the variable its type).
struct LoopHeader {
    variable: String,
    open: usize,
    counts: Option<(i64, i64, String)>,
}

/// Replaces each `for (var i = A; i < B; i++) { body }` (or `<=`) whose bounds are
/// integer literals or module `const` integers with one block per iteration,
/// `{ let i = A; body } { let i = A + 1; body } ...`, the same statements in the same
/// order. A loop whose body leaves early (`break`, `continue`), changes its variable
/// or runs more than `MAX_UNROLLED_ITERATIONS` times stays a loop; the loops inside
/// any loop are unrolled first.
fn unroll_loops(code: &str) -> String {
    let constants = integer_constants(code);
    unroll_within(code, &constants)
}

/// Module-scope `const NAME = 3;` or `const NAME: u32 = 3u;` integers, with the
/// literal suffix their type takes.
fn integer_constants(code: &str) -> HashMap<String, (i64, String)> {
    let mut constants = HashMap::new();
    for line in code.lines() {
        let Some(rest) = line.strip_prefix("const ") else {
            continue;
        };
        let Some((left, value)) = rest.trim_end().trim_end_matches(';').split_once('=') else {
            continue;
        };
        let (name, declared) = left.split_once(':').unwrap_or((left, ""));
        let Some((value, suffix)) = integer_literal(value.trim()) else {
            continue;
        };
        let suffix = match declared.trim() {
            "u32" => "u".to_owned(),
            "i32" | "" => suffix,
            _ => continue,
        };
        constants.insert(name.trim().to_owned(), (value, suffix));
    }
    constants
}

/// An integer literal's value and its type suffix (`""`, `"i"` or `"u"`).
fn integer_literal(text: &str) -> Option<(i64, String)> {
    let (digits, suffix) = match text.strip_suffix(['i', 'u']) {
        Some(digits) => (digits, text[digits.len()..].to_owned()),
        None => (text, String::new()),
    };
    digits.parse().ok().map(|value| (value, suffix))
}

fn unroll_within(code: &str, constants: &HashMap<String, (i64, String)>) -> String {
    let bytes = code.as_bytes();
    let mut out = String::with_capacity(code.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        if !word_at(bytes, i, "for") {
            i += 1;
            continue;
        }
        let Some(header) = loop_header(code, i, constants) else {
            i += 3;
            continue;
        };
        let end = block_end(bytes, header.open);
        let body = unroll_within(&code[header.open + 1..end - 1], constants);
        out.push_str(&code[copied..i]);
        match header.counts {
            Some((from, to, suffix))
                if to - from <= MAX_UNROLLED_ITERATIONS
                    && !leaves_early(&body)
                    && !assigns(&body, &header.variable) =>
            {
                for value in from..to {
                    out.push_str(&format!(
                        "{{ let {} = {value}{suffix};{body}}}\n",
                        header.variable
                    ));
                }
            }
            _ => {
                out.push_str(&code[i..=header.open]);
                out.push_str(&body);
                out.push('}');
            }
        }
        copied = end;
        i = end;
    }
    out.push_str(&code[copied..]);
    out
}

/// The header of the `for` statement at `at`, `None` if it is not one this reads.
fn loop_header(
    code: &str,
    at: usize,
    constants: &HashMap<String, (i64, String)>,
) -> Option<LoopHeader> {
    let bytes = code.as_bytes();
    let paren = skip_space(bytes, at + 3);
    if bytes.get(paren) != Some(&b'(') {
        return None;
    }
    let close = paren + bytes[paren..].iter().position(|&b| b == b')')?;
    let open = skip_space(bytes, close + 1);
    if bytes.get(open) != Some(&b'{') {
        return None;
    }
    let parts: Vec<&str> = code[paren + 1..close].split(';').map(str::trim).collect();
    let [init, condition, step] = parts[..] else {
        return None;
    };
    let (declaration, start) = init.strip_prefix("var ")?.split_once('=')?;
    let (variable, declared) = declaration.split_once(':').unwrap_or((declaration, ""));
    let variable = variable.trim().to_owned();
    let value = |text: &str| {
        let text = text.trim();
        integer_literal(text).or_else(|| constants.get(text).cloned())
    };
    let counts = (|| {
        let (from, suffix) = value(start)?;
        let suffix = match declared.trim() {
            "" => suffix,
            "i32" => String::new(),
            "u32" => "u".to_owned(),
            _ => return None,
        };
        let bound = condition.strip_prefix(variable.as_str())?.trim_start();
        let to = if let Some(last) = bound.strip_prefix("<=") {
            value(last)?.0 + 1
        } else {
            value(bound.strip_prefix('<')?)?.0
        };
        let steps = [format!("{variable}++"), format!("{variable} += 1")];
        (steps.iter().any(|s| s == step) && (suffix != "u" || from >= 0)).then_some((
            from,
            to.max(from),
            suffix,
        ))
    })();
    Some(LoopHeader {
        variable,
        open,
        counts,
    })
}

/// Whether `body` holds a `break` or `continue` (of its loop or one left inside it).
fn leaves_early(body: &str) -> bool {
    let bytes = body.as_bytes();
    (0..bytes.len()).any(|i| word_at(bytes, i, "break") || word_at(bytes, i, "continue"))
}

/// Whether `body` assigns `variable` or takes its address.
fn assigns(body: &str, variable: &str) -> bool {
    let bytes = body.as_bytes();
    (0..bytes.len()).any(|i| {
        if !word_at(bytes, i, variable) {
            return false;
        }
        let before = body[..i].trim_end();
        let after = body[i + variable.len()..].trim_start();
        let assigned = after.starts_with("++")
            || after.starts_with("--")
            || ["+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>="]
                .iter()
                .any(|op| after.starts_with(op))
            || (after.starts_with('=') && !after.starts_with("=="));
        assigned || (before.ends_with('&') && !before.ends_with("&&"))
    })
}

/// Removes functions no entry point (`@vertex`, `@fragment`, `@compute`) reaches.
fn prune_functions(code: &str) -> String {
    struct Function {
        name: String,
        start: usize,
        end: usize,
        entry: bool,
    }
    let bytes = code.as_bytes();
    let mut functions = Vec::new();
    let mut depth = 0;
    let mut item_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    item_start = i + 1;
                }
            }
            b';' if depth == 0 => item_start = i + 1,
            _ if depth == 0 && word_at(bytes, i, "fn") => {
                let name_start = skip_space(bytes, i + 2);
                let name_end = ident_end(bytes, name_start);
                let open = i + bytes[i..].iter().position(|&b| b == b'{').unwrap();
                let end = block_end(bytes, open);
                let attributes = &code[item_start..i];
                functions.push(Function {
                    name: code[name_start..name_end].to_owned(),
                    start: item_start,
                    end,
                    entry: ["@vertex", "@fragment", "@compute"]
                        .iter()
                        .any(|stage| attributes.contains(stage)),
                });
                i = end;
                item_start = end;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    // A call is a whole identifier followed by `(`; each body is scanned once and
    // its identifiers looked up, rather than searched once per function.
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, function) in functions.iter().enumerate() {
        by_name.entry(&function.name).or_default().push(index);
    }
    let calls = |function: &Function| -> Vec<usize> {
        let body = &bytes[function.start..function.end];
        let mut callees = Vec::new();
        let mut at = 0;
        while at < body.len() {
            if !is_ident(body[at]) {
                at += 1;
                continue;
            }
            let end = ident_end(body, at);
            if body.get(end) == Some(&b'(')
                && let Some(indices) = std::str::from_utf8(&body[at..end])
                    .ok()
                    .and_then(|name| by_name.get(name))
            {
                callees.extend_from_slice(indices);
            }
            at = end;
        }
        callees
    };
    let mut reached: Vec<bool> = functions.iter().map(|f| f.entry).collect();
    let mut pending: Vec<usize> = (0..functions.len()).filter(|&i| reached[i]).collect();
    while let Some(index) = pending.pop() {
        for callee in calls(&functions[index]) {
            if !std::mem::replace(&mut reached[callee], true) {
                pending.push(callee);
            }
        }
    }
    let mut out = String::with_capacity(code.len());
    let mut copied = 0;
    for (function, reached) in functions.iter().zip(reached) {
        if !reached {
            out.push_str(&code[copied..function.start]);
            copied = function.end;
        }
    }
    out.push_str(&code[copied..]);
    out
}

/// A module-scope item: a declaration with its attributes, or a directive.
struct Item<'a> {
    text: &'a str,
    /// The name it declares; directives (`enable`, `diagnostic`) declare none.
    name: Option<&'a str>,
    /// Whether it is a `@vertex`, `@fragment` or `@compute` function.
    entry: bool,
}

/// The module-scope items of `code` in order, and the text after the last one.
fn module_items(code: &str) -> (Vec<Item<'_>>, &str) {
    let bytes = code.as_bytes();
    let mut items = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        let end = match byte {
            b'{' => {
                depth += 1;
                false
            }
            b'}' => {
                depth -= 1;
                depth == 0
            }
            b';' => depth == 0,
            _ => false,
        };
        if end {
            items.push(item(&code[start..=i]));
            start = i + 1;
        }
    }
    (items, &code[start..])
}

fn item(text: &str) -> Item<'_> {
    let bytes = text.as_bytes();
    let mut i = skip_space(bytes, 0);
    let mut entry = false;
    // Attributes: `@name` and an optional parenthesized argument list.
    while bytes.get(i) == Some(&b'@') {
        let end = ident_end(bytes, i + 1);
        entry |= matches!(&text[i + 1..end], "vertex" | "fragment" | "compute");
        i = skip_space(bytes, end);
        if bytes.get(i) == Some(&b'(') {
            i += bytes[i..]
                .iter()
                .position(|&b| b == b')')
                .map_or(0, |at| at + 1);
            i = skip_space(bytes, i);
        }
    }
    let keyword_end = ident_end(bytes, i);
    let declares = matches!(
        &text[i..keyword_end],
        "fn" | "struct" | "const" | "var" | "override" | "alias"
    );
    let mut at = skip_space(bytes, keyword_end);
    if &text[i..keyword_end] == "var" && bytes.get(at) == Some(&b'<') {
        at = skip_space(
            bytes,
            at + bytes[at..].iter().position(|&b| b == b'>').unwrap_or(0) + 1,
        );
    }
    let name = declares.then(|| &text[at..ident_end(bytes, at)]);
    Item { text, name, entry }
}

/// The identifiers `text` mentions, numeric literal suffixes (`1u`, `2e3`) aside.
fn identifiers(text: &str) -> impl Iterator<Item = &str> {
    let bytes = text.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < bytes.len() {
            let start = i;
            if bytes[i].is_ascii_digit() {
                // A number with whatever suffix or exponent it carries.
                while i < bytes.len() && (is_ident(bytes[i]) || bytes[i] == b'.') {
                    i += 1;
                }
            } else if is_ident(bytes[i]) {
                i = ident_end(bytes, i);
                return Some(&text[start..i]);
            } else {
                i += 1;
            }
        }
        None
    })
}

/// `code` with `entry` as its only entry point and nothing that entry point does not
/// reach: no other entry points and none of the functions, structs, constants,
/// bindings or aliases only they use.
///
/// WebKit compiles each stage of each pipeline into a Metal library of its own (about
/// 0.1 s apiece on a cold shader cache, one at a time), from a translation of the
/// whole module it is given. Metal caches libraries by their source, so stages with
/// the same code share one compile only when their modules are the same text: the
/// vertex stage of most surface variants is, once their fragment code is gone.
pub fn entry_point_source(code: &str, entry: &str) -> String {
    let code = strip_comments(code);
    let (items, tail) = module_items(&code);
    let mut by_name: HashMap<&str, usize> = HashMap::new();
    for (index, item) in items.iter().enumerate() {
        if let Some(name) = item.name {
            by_name.insert(name, index);
        }
    }
    let mut kept: Vec<bool> = items
        .iter()
        .map(|item| item.name.is_none() || (item.entry && item.name == Some(entry)))
        .collect();
    assert!(
        items
            .iter()
            .any(|item| item.entry && item.name == Some(entry)),
        "no entry point {entry}"
    );
    let mut pending: Vec<usize> = (0..items.len()).filter(|&i| kept[i]).collect();
    while let Some(index) = pending.pop() {
        for word in identifiers(items[index].text) {
            if let Some(&used) = by_name.get(word)
                && !items[used].entry
                && !std::mem::replace(&mut kept[used], true)
            {
                pending.push(used);
            }
        }
    }
    let mut out = String::with_capacity(code.len());
    for (item, kept) in items.iter().zip(kept) {
        if kept {
            out.push_str(item.text);
        }
    }
    out.push_str(tail);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn squash(code: &str) -> String {
        code.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn keeps_the_taken_side_of_flag_branches() {
        let code = "fn f() { if A { a(); } if !A { b(); } if B { c(); } else { d(); } }";
        let flags = [("A", true), ("B", false)];
        assert_eq!(
            squash(&fold_flag_branches(code, &flags)),
            "fn f() { { a(); } { d(); } }"
        );
    }

    #[test]
    fn folds_else_if_chains_and_nested_branches() {
        let code = "fn f() { if !F { if D { x(); } else if B { y(); } } if T { if C { p(); } else if v < 1.0 { q(); } } }";
        let flags = [
            ("F", false),
            ("D", false),
            ("B", true),
            ("T", true),
            ("C", false),
        ];
        assert_eq!(
            squash(&fold_flag_branches(code, &flags)),
            "fn f() { { { y(); } } { if v < 1.0 { q(); } } }"
        );
    }

    #[test]
    fn leaves_runtime_conditions_and_expressions_alone() {
        let code = "fn f() { if x > 0.0 { a(); } else if A { b(); } let c = select(1.0, 2.0, A); }";
        assert_eq!(fold_flag_branches(code, &[("A", false)]), code);
    }

    #[test]
    fn prunes_functions_no_entry_point_reaches() {
        let code = "fn used() -> f32 { return helper(); }\nfn helper() -> f32 { return 1.0; }\nfn unused() -> f32 { return helper(); }\n@fragment\nfn fs() -> @location(0) vec4f { return vec4f(used()); }\n";
        let pruned = prune_functions(code);
        assert!(pruned.contains("fn used(") && pruned.contains("fn helper("));
        assert!(!pruned.contains("fn unused("));
        assert!(pruned.contains("@fragment\nfn fs("));
    }

    #[test]
    fn a_call_is_a_whole_identifier_before_a_parenthesis() {
        let code = "fn helper() -> f32 { return 1.0; }\nfn helper_x() -> f32 { return 2.0; }\nfn my_helper() -> f32 { return 3.0; }\nfn named() -> f32 { return 4.0; }\n@fragment\nfn fs() -> @location(0) vec4f { let named = 1.0; return vec4f(helper_x() + my_helper (), named, 0.0, 1.0); }\n";
        let pruned = prune_functions(code);
        assert!(pruned.contains("fn helper_x(") && pruned.contains("@fragment\nfn fs("));
        // `helper` only as part of longer names, `my_helper` not followed by `(`
        // and `named` only as a variable: none of them is called.
        assert!(!pruned.contains("fn helper("));
        assert!(!pruned.contains("fn my_helper("));
        assert!(!pruned.contains("fn named("));
    }

    #[test]
    fn unrolls_counted_loops_into_one_block_per_iteration() {
        let code = "fn f() -> f32 { var s = 0.0; for (var j = -1; j <= 1; j++) { for (var i = 0; i < 2; i++) { s += f32(i * j); } } return s; }";
        let unrolled = unroll_loops(code);
        assert!(!unrolled.contains("for ("));
        assert_eq!(unrolled.matches("let j = ").count(), 3);
        assert_eq!(unrolled.matches("let i = ").count(), 6);
        assert_eq!(unrolled.matches("s += f32(i * j);").count(), 6);
        let order: Vec<&str> = unrolled
            .split("let ")
            .skip(1)
            .map(|declaration| declaration.split(';').next().unwrap())
            .collect();
        assert_eq!(
            order,
            [
                "j = -1", "i = 0", "i = 1", "j = 0", "i = 0", "i = 1", "j = 1", "i = 0", "i = 1"
            ]
        );
    }

    #[test]
    fn unrolls_to_constant_bounds_with_their_type() {
        let code = "const N: u32 = 3u;\nconst M = 2;\nfn f() { for (var k = 0u; k < N; k++) { a(k); } for (var i = 0; i < M; i++) { b(i); } }";
        let unrolled = unroll_loops(code);
        assert!(!unrolled.contains("for ("));
        assert!(unrolled.contains("let k = 0u;") && unrolled.contains("let k = 2u;"));
        assert!(!unrolled.contains("let k = 3u;") && unrolled.contains("let i = 1;"));
    }

    #[test]
    fn keeps_loops_it_cannot_unroll() {
        let code = "fn f(n: i32) { for (var i = 0; i < n; i++) { a(); } for (var i = 0; i < 4; i++) { if b() { continue; } } for (var i = 0; i < 4; i++) { if b() { break; } } for (var i = 0; i < 4; i++) { i += 1; } for (var i = 0; i < 40; i++) { c(); } for (var i = 0; i < 4; i++) { for (var k = 0; k < 2; k++) { if d(k) { break; } } } }";
        let unrolled = unroll_loops(code);
        assert_eq!(unrolled.matches("for (").count(), 7);
        assert!(!unrolled.contains("let "));
    }

    #[test]
    fn unrolls_the_loops_inside_a_loop_it_keeps() {
        let code = "fn f(n: i32) { for (var i = 0; i < n; i++) { for (var k = 0; k < 2; k++) { a(k); } } }";
        let unrolled = unroll_loops(code);
        assert_eq!(unrolled.matches("for (").count(), 1);
        assert!(unrolled.contains("let k = 0;") && unrolled.contains("let k = 1;"));
    }

    #[test]
    fn an_entry_point_source_keeps_only_what_that_entry_reaches() {
        let code = "enable f16;\nconst SCALE: f32 = 2.0;\nconst TINT = vec3f(1.0);\nalias Color = vec4f;\nstruct Out {\n    @builtin(position) clip: vec4f,\n    @location(0) uv: vec2f,\n}\n@group(0) @binding(0) var<uniform> scale: vec4f;\n@group(0) @binding(1) var tint_texture: texture_2d<f32>;\nfn place(p: vec2f) -> vec4f { return vec4f(p * SCALE, 0.0, 1.0) * scale; }\nfn shade(uv: vec2f) -> Color { return textureLoad(tint_texture, vec2i(uv), 0) * vec4f(TINT, 1.0); }\n@vertex\nfn vs(@location(0) p: vec2f) -> Out { return Out(place(p), p * 1e2); }\n@fragment\nfn fs(input: Out) -> @location(0) Color { return shade(input.uv); }\n";
        let vertex = entry_point_source(code, "vs");
        assert_eq!(
            vertex,
            "enable f16;\nconst SCALE: f32 = 2.0;\nstruct Out {\n    @builtin(position) clip: vec4f,\n    @location(0) uv: vec2f,\n}\n@group(0) @binding(0) var<uniform> scale: vec4f;\nfn place(p: vec2f) -> vec4f { return vec4f(p * SCALE, 0.0, 1.0) * scale; }\n@vertex\nfn vs(@location(0) p: vec2f) -> Out { return Out(place(p), p * 1e2); }\n"
        );
        let fragment = entry_point_source(code, "fs");
        assert!(fragment.contains("alias Color") && fragment.contains("var tint_texture"));
        assert!(fragment.contains("const TINT") && fragment.contains("struct Out"));
        assert!(
            !fragment.contains("SCALE")
                && !fragment.contains("fn vs(")
                && !fragment.contains("fn place(")
        );
        assert!(!fragment.contains("var<uniform> scale"));
    }

    #[test]
    fn strips_comments() {
        let code = "// head\nconst A: f32 = 1.0; // tail\n/* a /* nested */ block */\nfn f() {}\n";
        assert_eq!(strip_comments(code), "const A: f32 = 1.0;\nfn f() {}\n");
    }
}
