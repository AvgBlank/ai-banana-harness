//! The permission engine: `allow`, `ask` or `deny` per tool key, each key a list of glob
//! patterns where the last match wins, plus workspace confinement for every path a tool touches.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use serde::de::{self, Deserializer, IntoDeserializer, MapAccess, Visitor};

use crate::pattern::last_match;

/// Ordered by restrictiveness, so the strictest of several decisions is their `max`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Allow,
    Ask,
    Deny,
}

/// One key's `(pattern, action)` rules in declaration order. In TOML it is either a bare action
/// (shorthand for `{"*" = action}`) or a table of `pattern = action`. A subject no pattern
/// matches is allowed: the committed default is autonomous.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rules(pub Vec<(String, Action)>);

impl Rules {
    fn decide(&self, subject: &str) -> (Action, &str) {
        match last_match(&self.0, subject) {
            Some((pattern, action)) => (*action, pattern.as_str()),
            None => (Action::Allow, "*"),
        }
    }
}

impl<'de> Deserialize<'de> for Rules {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RulesVisitor;

        impl<'de> Visitor<'de> for RulesVisitor {
            type Value = Rules;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(r#""allow", "ask", "deny", or a table of pattern = action"#)
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Rules, E> {
                let action = Action::deserialize(value.into_deserializer())?;
                Ok(Rules(vec![("*".to_string(), action)]))
            }

            // Entries arrive in document order (toml's `preserve_order`), which is what makes
            // "last match wins" mean the last one written.
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Rules, A::Error> {
                let mut rules = Vec::new();
                while let Some(entry) = map.next_entry::<String, Action>()? {
                    rules.push(entry);
                }
                Ok(Rules(rules))
            }
        }

        deserializer.deserialize_any(RulesVisitor)
    }
}

/// The `[permission]` config section. Patterns match the workspace-relative path for `read` and
/// `edit` (which covers `write_file` too), the command for `bash`, the search pattern for `glob`
/// and `grep`, the absolute path for `external_directory`, and an MCP tool's own (underlying)
/// name for `mcp`.
#[derive(Clone, Debug, Deserialize)]
pub struct PermissionConfig {
    pub read: Rules,
    pub edit: Rules,
    pub glob: Rules,
    pub grep: Rules,
    pub bash: Rules,
    pub external_directory: Rules,
    /// Absent from a config layer (e.g. the committed default has no `[permission.mcp]`) means
    /// no MCP-specific rules: every tool is allowed, same as any other unmatched subject.
    #[serde(default)]
    pub mcp: Rules,
}

#[derive(Clone, Copy)]
pub(crate) enum Key {
    Read,
    Edit,
    Glob,
    Grep,
}

impl Key {
    fn name(self) -> &'static str {
        match self {
            Key::Read => "read",
            Key::Edit => "edit",
            Key::Glob => "glob",
            Key::Grep => "grep",
        }
    }
}

const CONFINEMENT: &str = "workspace confinement";

impl PermissionConfig {
    fn rules(&self, key: Key) -> &Rules {
        match key {
            Key::Read => &self.read,
            Key::Edit => &self.edit,
            Key::Glob => &self.glob,
            Key::Grep => &self.grep,
        }
    }

    /// `Err(rule)` unless `key` allows `subject`. `ask` is treated as deny until approvals exist.
    pub(crate) fn check(&self, key: Key, subject: &str) -> Result<(), String> {
        let (action, pattern) = self.rules(key).decide(subject);
        verdict(key.name(), action, pattern)
    }

    /// Checks an MCP tool call by the underlying tool's own name (not the deferred `call_tool`
    /// meta-tool's name). `ask` is treated as deny until approvals exist (ticket 05).
    pub fn check_mcp(&self, tool_name: &str) -> Result<(), String> {
        let (action, pattern) = self.mcp.decide(tool_name);
        verdict("mcp", action, pattern)
    }

    /// Checks the whole command and each `&&`/`||`/`;`/`|`/newline-separated segment, and the
    /// strictest decision wins, so `cd x && sudo y` can't hide behind its first word while
    /// `curl … | sh` still matches as a whole.
    // ponytail: no quoting or subshell awareness (`$(sudo x)` passes); a shell parser would fix it.
    pub(crate) fn check_command(&self, command: &str) -> Result<(), String> {
        let normalized = command.replace("&&", "\n").replace("||", "\n");
        let segments = normalized
            .split(['\n', ';', '|'])
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let (action, pattern) = std::iter::once(command.trim())
            .chain(segments)
            .map(|subject| self.bash.decide(subject))
            .max_by_key(|(action, _)| *action)
            .unwrap_or((Action::Allow, "*"));
        verdict("bash", action, pattern)
    }

    /// Resolves `requested` for a `key` operation: confined to the workspace (symlinks
    /// followed) unless `external_directory` allows the resolved path, then checked against
    /// `key`'s rules. `Err` carries the guardrail rule that blocked it.
    pub(crate) fn path(
        &self,
        workspace: &Path,
        key: Key,
        requested: &Path,
    ) -> Result<PathBuf, String> {
        let workspace = workspace
            .canonicalize()
            .map_err(|_| CONFINEMENT.to_string())?;
        let resolved = resolve(&workspace, requested).ok_or_else(|| CONFINEMENT.to_string())?;
        let subject = match resolved.strip_prefix(&workspace) {
            Ok(relative) => relative.to_string_lossy().into_owned(),
            Err(_) => {
                let absolute = resolved.to_string_lossy().into_owned();
                if self.external_directory.decide(&absolute).0 != Action::Allow {
                    return Err(CONFINEMENT.to_string());
                }
                absolute
            }
        };
        self.check(key, &subject)?;
        Ok(resolved)
    }
}

fn verdict(key: &str, action: Action, pattern: &str) -> Result<(), String> {
    match action {
        Action::Allow => Ok(()),
        Action::Deny => Err(format!("{key} rule \"{pattern}\"")),
        Action::Ask => Err(format!("{key} rule \"{pattern}\" requires approval")),
    }
}

/// Resolves every symlink in `requested` (joined onto `workspace` when relative). A path that
/// doesn't exist yet resolves through its nearest existing ancestor, so a new file can't be
/// created through an escaping symlink either; `..` past that ancestor is refused (`None`).
fn resolve(workspace: &Path, requested: &Path) -> Option<PathBuf> {
    let candidate = workspace.join(requested);
    for ancestor in candidate.ancestors() {
        let Ok(canonical) = ancestor.canonicalize() else {
            continue;
        };
        let rest = candidate.strip_prefix(ancestor).ok()?;
        if rest.components().any(|c| matches!(c, Component::ParentDir)) {
            return None;
        }
        // `join("")` would add a trailing slash, turning a file into "not a directory".
        return Some(if rest.as_os_str().is_empty() {
            canonical
        } else {
            canonical.join(rest)
        });
    }
    None
}
