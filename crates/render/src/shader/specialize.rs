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

/// `code` with every `if FLAG { ... } [else ...]` whose condition is one of `flags`
/// (optionally negated) replaced by the taken block, then without comments,
/// unreachable functions and the declarations of flags nothing reads any more (so
/// variants that fold to the same code are the same text for the browser's caches).
pub fn specialize(code: &str, flags: &[(&str, bool)]) -> String {
    let code = strip_comments(code);
    let code = fold_flag_branches(&code, flags);
    let mut code = prune_functions(&code);
    for (name, value) in flags {
        let declaration = format!("const {name}: bool = {value};\n");
        if let Some(at) = code.find(&declaration) {
            let bytes = code.as_bytes();
            let used = (0..bytes.len())
                .any(|i| (i < at || i >= at + declaration.len()) && word_at(bytes, i, name));
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
fn word_at(bytes: &[u8], i: usize, word: &str) -> bool {
    bytes[i..].starts_with(word.as_bytes())
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
    let calls = |function: &Function| -> Vec<usize> {
        let body = &bytes[function.start..function.end];
        functions
            .iter()
            .enumerate()
            .filter(|(_, callee)| {
                (0..body.len()).any(|at| {
                    word_at(body, at, &callee.name)
                        && body.get(at + callee.name.len()) == Some(&b'(')
                })
            })
            .map(|(index, _)| index)
            .collect()
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
    fn strips_comments() {
        let code = "// head\nconst A: f32 = 1.0; // tail\n/* a /* nested */ block */\nfn f() {}\n";
        assert_eq!(strip_comments(code), "const A: f32 = 1.0;\nfn f() {}\n");
    }
}
