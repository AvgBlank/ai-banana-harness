//! A small glob-style matcher for guardrail rules (e.g. `.env*`). `*` matches any run of
//! characters, including none; every other character must match literally. Good enough for the
//! fixed guardrail rule set — reach for `globset` (already used by the `glob` tool) if a rule
//! ever needs character classes or `**`.

pub fn glob_match(pattern: &str, text: &str) -> bool {
    fn match_bytes(pattern: &[u8], text: &[u8]) -> bool {
        match pattern.first() {
            None => text.is_empty(),
            Some(b'*') => {
                match_bytes(&pattern[1..], text)
                    || (!text.is_empty() && match_bytes(pattern, &text[1..]))
            }
            Some(&c) => !text.is_empty() && text[0] == c && match_bytes(&pattern[1..], &text[1..]),
        }
    }
    match_bytes(pattern.as_bytes(), text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::glob_match;

    // Table-driven per the ticket's one permitted seam exception: the guardrail pattern
    // matcher's combinatorics, not tool behaviour.
    #[test]
    fn combinatorics() {
        let cases: &[(&str, &str, bool)] = &[
            (".env*", ".env", true),
            (".env*", ".env.local", true),
            (".env*", ".environment", true),
            (".env*", "env", false),
            (".env*", ".ENV", false),
            (".env*", "foo.env", false),
            ("*", "", true),
            ("*", "anything", true),
            ("*.txt", "notes.txt", true),
            ("*.txt", "notes.txt.bak", false),
            ("a*b*c", "abc", true),
            ("a*b*c", "aXbYc", true),
            ("a*b*c", "aXbY", false),
            ("sudo *", "sudo rm -rf /", true),
            ("sudo *", "sudoku", false),
            ("", "", true),
            ("", "x", false),
            ("exact", "exact", true),
            ("exact", "exacter", false),
        ];
        for (pattern, text, expected) in cases {
            assert_eq!(
                glob_match(pattern, text),
                *expected,
                "glob_match({pattern:?}, {text:?})"
            );
        }
    }
}
