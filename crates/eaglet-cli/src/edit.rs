//! Small, careful edits to existing Rust files: add a `mod`, a `use`, or an item to a
//! `#[module(...)]` list. `rustfmt` puts things in order afterwards.

/// Add `line` (for example `pub mod products;`) after the last `mod` line, or after the leading
/// doc comment when there is none. Nothing changes when the line is already there.
#[must_use]
pub fn add_mod(src: &str, line: &str) -> String {
    let is_mod = |l: &str| l.starts_with("pub mod ") || l.starts_with("mod ");
    insert_after_last(src, line, is_mod)
}

/// Add `line` (for example `use crate::products::ProductsModule;`) after the last top-level
/// `use` line.
///
/// A `pub use` goes with the other `pub use` lines, or starts its own group after the `use` lines,
/// so `rustfmt` keeps re-exports apart from imports.
#[must_use]
pub fn add_use(src: &str, line: &str) -> String {
    let is_pub_use = |l: &str| l.starts_with("pub use ");
    let is_use = |l: &str| l.starts_with("use ");
    if is_pub_use(line) {
        if src.lines().any(is_pub_use) {
            return insert_after_last(src, line, is_pub_use);
        }
        if src.lines().any(is_use) {
            return insert_after_last(src, &format!("\n{line}"), is_use);
        }
    } else if src.lines().any(is_use) {
        return insert_after_last(src, line, is_use);
    }
    add_mod(src, line)
}

fn insert_after_last(src: &str, line: &str, matches: impl Fn(&str) -> bool) -> String {
    if src.lines().any(|l| l.trim() == line.trim()) {
        return src.to_owned();
    }
    let mut out: Vec<String> = src.lines().map(str::to_owned).collect();
    if let Some(i) = out.iter().rposition(|l| matches(l)) {
        out.insert(i + 1, line.to_owned());
    } else {
        // after the `//!` block, separated by a blank line
        let docs = out.iter().take_while(|l| l.starts_with("//!")).count();
        if docs == 0 {
            out.insert(0, line.to_owned());
        } else {
            out.insert(docs, String::new());
            out.insert(docs + 1, line.to_owned());
        }
    }
    finish(&out, src)
}

fn finish(lines: &[String], original: &str) -> String {
    let mut out = lines.join("\n");
    if original.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// The index just after the bracket that closes the one opened at `open`, skipping strings.
fn closing(src: &str, open: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let mut in_str = false;
    let mut i = open;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if b == b'\\' {
                i += 1;
            } else if b == b'"' {
                in_str = false;
            }
        } else {
            match b {
                b'"' => in_str = true,
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// From `from`, skip spaces, expect `=`, skip spaces, expect `[`: the index of that `[`.
fn bracket_after(bytes: &[u8], from: usize) -> Option<usize> {
    let skip = |mut i: usize| {
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        i
    };
    let eq = skip(from);
    (bytes.get(eq) == Some(&b'=')).then_some(())?;
    let open = skip(eq + 1);
    (bytes.get(open) == Some(&b'[')).then_some(open)
}

/// Where `key = [` starts at the top level of the `#[module(...)]` arguments in `args`, and the
/// index of its `[`.
fn find_key(args: &str, key: &str) -> Option<usize> {
    let bytes = args.as_bytes();
    let mut depth = 0usize;
    let mut in_str = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            in_str = b != b'"';
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ if depth == 0 && args[i..].starts_with(key) => {
                let starts_word =
                    i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
                if starts_word && let Some(bracket) = bracket_after(bytes, i + key.len()) {
                    return Some(bracket);
                }
            }
            _ => {}
        }
    }
    None
}

/// Add `item` to `key = [...]` in the file's `#[module(...)]`, keeping the list's layout. When the
/// list is missing it is added. `None` when the file has no `#[module(...)]`.
#[must_use]
pub fn add_to_module_list(src: &str, key: &str, item: &str) -> Option<String> {
    let attr = src.find("#[module(")?;
    let open = attr + "#[module".len();
    let close = closing(src, open)?;
    let args = &src[open + 1..close - 1];
    let Some(rel) = find_key(args, key) else {
        let multiline = args.starts_with('\n');
        let insert = if multiline {
            format!("\n    {key} = [\n        {item},\n    ],")
        } else if args.trim().is_empty() {
            format!("{key} = [{item}]")
        } else {
            format!("{key} = [{item}], ")
        };
        return Some(format!("{}{insert}{}", &src[..=open], &src[open + 1..]));
    };
    let bracket = open + 1 + rel;
    let end = closing(src, bracket)? - 1;
    let inner = &src[bracket + 1..end];
    let present = inner
        .split(|c: char| c == ',' || c.is_whitespace())
        .any(|w| w == item);
    if present {
        return Some(src.to_owned());
    }
    let trimmed = inner.trim_end();
    let tail = &inner[trimmed.len()..];
    let new_inner = if trimmed.trim().is_empty() {
        item.to_owned()
    } else if trimmed.contains('\n') {
        let indent: String = trimmed
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        let sep = if trimmed.ends_with(',') { "" } else { "," };
        format!("{trimmed}{sep}\n{indent}{item},{tail}")
    } else {
        let sep = if trimmed.ends_with(',') { "" } else { "," };
        format!("{trimmed}{sep} {item}{tail}")
    };
    Some(format!("{}{new_inner}{}", &src[..=bracket], &src[end..]))
}

#[cfg(test)]
mod tests {
    use super::{add_mod, add_to_module_list, add_use};

    const APP: &str = "//! Root.\n\nuse eaglet::prelude::*;\n\nuse crate::health::HealthModule;\n\n#[module(\n    imports = [\n        ConfigModule,\n        HealthModule,\n    ]\n)]\npub struct AppModule;\n";

    #[test]
    fn adds_to_a_multiline_list_in_its_style() {
        let out = add_to_module_list(APP, "imports", "ProductsModule").unwrap_or_default();
        assert!(
            out.contains("        HealthModule,\n        ProductsModule,\n    ]\n)]"),
            "{out}"
        );
        let again = add_to_module_list(&out, "imports", "ProductsModule").unwrap_or_default();
        assert_eq!(again, out, "adding twice changes nothing");
    }

    #[test]
    fn adds_to_one_line_and_empty_lists_and_missing_keys() {
        let one = "#[module(controllers = [A], providers = [])]\nstruct M;\n";
        let out = add_to_module_list(one, "controllers", "B").unwrap_or_default();
        assert!(out.contains("controllers = [A, B]"), "{out}");
        let out = add_to_module_list(&out, "providers", "S").unwrap_or_default();
        assert!(out.contains("providers = [S]"), "{out}");
        let out = add_to_module_list(
            "#[module(providers = [S])]\nstruct M;\n",
            "controllers",
            "C",
        )
        .unwrap_or_default();
        assert!(
            out.contains("#[module(controllers = [C], providers = [S])]"),
            "{out}"
        );
        let nested =
            "#[module(providers = [ProviderDef::value(Config { a: [1] })], exports = [Config])]";
        let out = add_to_module_list(nested, "providers", "Other").unwrap_or_default();
        assert!(out.contains("Config { a: [1] }), Other], exports"), "{out}");
    }

    #[test]
    fn no_module_attribute_is_reported() {
        assert_eq!(add_to_module_list("struct M;", "imports", "X"), None);
    }

    #[test]
    fn mods_and_uses_go_after_their_kind() {
        let lib = "//! App.\n\npub mod app_module;\npub mod health;\n";
        let out = add_mod(lib, "pub mod products;");
        assert_eq!(
            out,
            "//! App.\n\npub mod app_module;\npub mod health;\npub mod products;\n"
        );
        assert_eq!(add_mod(&out, "pub mod products;"), out);
        let out = add_use(APP, "use crate::products::ProductsModule;");
        assert!(
            out.contains(
                "use crate::health::HealthModule;\nuse crate::products::ProductsModule;\n"
            )
        );
        let bare = "//! Docs.\n\n#[module(controllers = [])]\npub struct M;\n";
        assert!(add_mod(bare, "mod controller;").starts_with("//! Docs.\n\nmod controller;\n"));
        let module = "mod controller;\n\nuse eaglet::prelude::*;\n\n#[module()]\n";
        let out = add_use(module, "pub use controller::C;");
        assert!(
            out.contains("use eaglet::prelude::*;\n\npub use controller::C;\n\n#[module"),
            "{out}"
        );
        let out = add_use(&out, "pub use service::S;");
        assert!(
            out.contains("pub use controller::C;\npub use service::S;\n"),
            "{out}"
        );
    }
}
