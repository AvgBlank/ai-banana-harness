//! Scans configured directories for `<dir>/SKILL.md` files and parses their frontmatter.
//!
//! Each configured path may be relative to the workspace, `~`-relative, or absolute.
//! Invalid names and skills past the limit are dropped with a warning rather than failing the turn.

use std::path::{Path, PathBuf};

use etcetera::BaseStrategy;

/// A discovered skill, ready to be rendered into the system prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// Absolute path to the skill's directory (containing `SKILL.md`).
    pub path: PathBuf,
}

/// The result of a scan: the skills to render, plus warnings for anything skipped.
#[derive(Debug, Default)]
pub struct LoadedSkills {
    pub skills: Vec<Skill>,
    pub warnings: Vec<String>,
}

const MAX_SKILLS: usize = 50;

/// Scans `paths` (each `<path>/*/SKILL.md`) for skills. Relative paths resolve against
/// `workspace`; a `~` prefix resolves against the home directory when one can be found.
pub fn load_skills(paths: &[String], workspace: &Path) -> LoadedSkills {
    let home = etcetera::choose_base_strategy()
        .ok()
        .map(|base| base.home_dir().to_path_buf());

    let mut result = LoadedSkills::default();
    for raw in paths {
        let Some(dir) = resolve_path(raw, workspace, home.as_deref()) else {
            result.warnings.push(format!(
                "skipping skills path '{raw}': no home directory to resolve `~`"
            ));
            continue;
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entry_paths: Vec<PathBuf> =
            entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        entry_paths.sort();
        for entry in entry_paths {
            scan_skill_dir(&entry, &mut result);
        }
    }
    result
}

fn scan_skill_dir(dir: &Path, result: &mut LoadedSkills) {
    if !dir.is_dir() {
        return;
    }
    let skill_md = dir.join("SKILL.md");
    let Ok(content) = std::fs::read_to_string(&skill_md) else {
        return;
    };
    let Some(front) = parse_frontmatter(&content) else {
        result.warnings.push(format!(
            "skipping {}: SKILL.md has no frontmatter",
            skill_md.display()
        ));
        return;
    };
    let (Some(name), Some(description)) = (front.name, front.description) else {
        result.warnings.push(format!(
            "skipping {}: SKILL.md frontmatter is missing `name` or `description`",
            skill_md.display()
        ));
        return;
    };
    if !is_valid_skill_name(&name) {
        result.warnings.push(format!(
            "skipping skill '{name}': name must match ^[A-Za-z0-9._-/:]+$"
        ));
        return;
    }
    if result.skills.len() >= MAX_SKILLS {
        result.warnings.push(format!(
            "skipping skill '{name}': more than {MAX_SKILLS} skills are configured"
        ));
        return;
    }
    result.skills.push(Skill {
        name,
        description,
        path: dir.to_path_buf(),
    });
}

fn resolve_path(raw: &str, workspace: &Path, home: Option<&Path>) -> Option<PathBuf> {
    if let Some(rest) = raw.strip_prefix("~/") {
        return home.map(|h| h.join(rest));
    }
    if raw == "~" {
        return home.map(Path::to_path_buf);
    }
    let path = Path::new(raw);
    Some(if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace.join(path)
    })
}

fn is_valid_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | ':'))
}

struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
}

/// A minimal `name:`/`description:` frontmatter reader: no nested structures or multi-line
/// values, which is all the skill contract needs.
fn parse_frontmatter(content: &str) -> Option<Frontmatter> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut name = None;
    let mut description = None;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some(value) = line.strip_prefix("name:") {
            name = Some(unquote(value));
        } else if let Some(value) = line.strip_prefix("description:") {
            description = Some(unquote(value));
        }
    }
    Some(Frontmatter { name, description })
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    let quoted = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
        });
    quoted.unwrap_or(trimmed).to_string()
}
