//! The spellings of one name: `order-items` → `OrderItems`, `OrderItem`, `order_items`, ...

/// Words Rust reserves; a module cannot be called one of them.
const KEYWORDS: [&str; 51] = [
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
    "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true", "type", "union",
    "unsafe", "use", "where", "while", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "try", "typeof", "unsized", "virtual",
];

/// One name in every spelling the templates need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Names {
    /// `OrderItems`
    pub pascal: String,
    /// `OrderItem`
    pub singular_pascal: String,
    /// `order_items`
    pub snake: String,
    /// `order-items`
    pub kebab: String,
    /// `order items`
    pub words: String,
    /// `order item`
    pub singular_words: String,
}

/// `item` from `items`, `category` from `categories`, `address` from `addresses`.
fn singular(word: &str) -> String {
    if let Some(stem) = word.strip_suffix("ies")
        && !stem.is_empty()
    {
        return format!("{stem}y");
    }
    for ending in ["sses", "shes", "ches", "xes", "zes"] {
        if word.ends_with(ending) {
            return word[..word.len() - 2].to_owned();
        }
    }
    if word.len() > 1 && word.ends_with('s') && !word.ends_with("ss") {
        return word[..word.len() - 1].to_owned();
    }
    word.to_owned()
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// Split `orderItems`, `OrderItems`, `order-items` and `order_items` into `["order", "items"]`.
fn split_words(name: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if c == '-' || c == '_' || c == ' ' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() && prev_lower && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        prev_lower = c.is_lowercase() || c.is_ascii_digit();
        current.extend(c.to_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

impl Names {
    /// Check `name` and spell it every way.
    ///
    /// # Errors
    /// When the name is empty, starts with a digit, has characters other than letters, digits,
    /// `-` and `_`, or is a Rust keyword.
    pub fn parse(name: &str) -> Result<Self, String> {
        let valid = name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid || !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return Err(format!(
                "`{name}` is not a usable name: start with a letter and use only letters, digits, - and _"
            ));
        }
        let words = split_words(name);
        let snake = words.join("_");
        if KEYWORDS.contains(&snake.as_str()) {
            return Err(format!(
                "`{name}` is a Rust keyword and cannot name a module; try `{name}s` or `my-{name}`"
            ));
        }
        let mut singular_words = words.clone();
        if let Some(last) = singular_words.last_mut() {
            *last = singular(last);
        }
        Ok(Self {
            pascal: words.iter().map(|w| capitalize(w)).collect(),
            singular_pascal: singular_words.iter().map(|w| capitalize(w)).collect(),
            snake,
            kebab: words.join("-"),
            words: words.join(" "),
            singular_words: singular_words.join(" "),
        })
    }

    /// `Shop API` style title from `shop-api`.
    #[must_use]
    pub fn title(&self) -> String {
        self.words
            .split(' ')
            .map(capitalize)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::Names;

    #[test]
    fn every_spelling() {
        let n = Names::parse("order-items").unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(n.pascal, "OrderItems");
        assert_eq!(n.singular_pascal, "OrderItem");
        assert_eq!(n.snake, "order_items");
        assert_eq!(n.kebab, "order-items");
        assert_eq!(n.words, "order items");
        assert_eq!(n.singular_words, "order item");
        assert_eq!(n.title(), "Order Items");
    }

    #[test]
    fn other_input_styles_and_plurals() {
        for (input, pascal, singular) in [
            ("orderItems", "OrderItems", "OrderItem"),
            ("OrderItems", "OrderItems", "OrderItem"),
            ("order_items", "OrderItems", "OrderItem"),
            ("categories", "Categories", "Category"),
            ("addresses", "Addresses", "Address"),
            ("boxes", "Boxes", "Box"),
            ("news", "News", "New"),
            ("staff", "Staff", "Staff"),
        ] {
            let n = Names::parse(input).unwrap_or_else(|e| unreachable!("{e}"));
            assert_eq!(
                (n.pascal.as_str(), n.singular_pascal.as_str()),
                (pascal, singular),
                "{input}"
            );
        }
    }

    #[test]
    fn bad_names_are_refused_with_a_reason() {
        assert!(Names::parse("type").is_err_and(|e| e.contains("keyword")));
        assert!(Names::parse("9lives").is_err_and(|e| e.contains("start with a letter")));
        assert!(Names::parse("a b").is_err());
        assert!(Names::parse("").is_err());
    }
}
