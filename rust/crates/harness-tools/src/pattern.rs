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

/// The last `(pattern, value)` rule whose pattern matches `text`: later rules override earlier
/// ones, so a broad `*` goes first and its exceptions after it.
pub fn last_match<'r, T>(rules: &'r [(String, T)], text: &str) -> Option<&'r (String, T)> {
    rules
        .iter()
        .rev()
        .find(|(pattern, _)| glob_match(pattern, text))
}

#[cfg(test)]
mod tests {
    use super::{glob_match, last_match};
    use crate::Action::{self, Allow, Ask, Deny};

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

    // Same permitted exception: last-match-wins across mixed allow/ask/deny rules.
    #[test]
    fn last_match_wins() {
        let rules: Vec<(String, Action)> = [
            ("*", Allow),
            ("git *", Ask),
            ("git push*", Deny),
            ("git push --dry-run*", Allow),
            ("rm *", Deny),
            ("rm -rf ./target*", Allow),
            ("*", Allow),
            ("sudo *", Deny),
        ]
        .into_iter()
        .map(|(p, a)| (p.to_string(), a))
        .collect();
        let cases: &[(&str, Option<(&str, Action)>)] = &[
            ("ls -la", Some(("*", Allow))),
            // The trailing `*` rule re-allows everything that isn't matched after it...
            ("git status", Some(("*", Allow))),
            ("git push origin main", Some(("*", Allow))),
            ("rm -rf /", Some(("*", Allow))),
            // ...while rules after it still win.
            ("sudo rm -rf /", Some(("sudo *", Deny))),
            ("sudo", Some(("*", Allow))),
        ];
        for (text, expected) in cases {
            let got = last_match(&rules, text).map(|(p, a)| (p.as_str(), *a));
            assert_eq!(got, *expected, "last_match(.., {text:?})");
        }

        // Without the trailing `*`, the specific rules decide, the most recent match first.
        let rules = &rules[..6];
        let cases: &[(&str, Option<(&str, Action)>)] = &[
            ("ls", Some(("*", Allow))),
            ("git status", Some(("git *", Ask))),
            ("git push origin main", Some(("git push*", Deny))),
            (
                "git push --dry-run origin",
                Some(("git push --dry-run*", Allow)),
            ),
            ("rm -rf /", Some(("rm *", Deny))),
            ("rm -rf ./target/debug", Some(("rm -rf ./target*", Allow))),
            ("rm -rf ./targets", Some(("rm -rf ./target*", Allow))),
        ];
        for (text, expected) in cases {
            let got = last_match(rules, text).map(|(p, a)| (p.as_str(), *a));
            assert_eq!(got, *expected, "last_match(.., {text:?})");
        }
        assert_eq!(last_match::<Action>(&[], "anything"), None);
        assert_eq!(last_match(&rules[1..3], "ls"), None);
    }
}
