//! What changed in the vault, as a hint: the watcher is the kit's, the rule of which paths matter
//! is the rules hub's own and stays here, so that the kit cannot be made to watch less.
//!
//! The rule sets are the folders under `<vault>/rulesets/`, and `<vault>/.rules.json` names what is
//! loaded from them. Everything else in the vault (`.git`, a README, tool state) is not a rule and
//! must not wake the scan: a scan that is woken by its own tools would never rest.

use std::path::Path;

pub use appkit_hub::fs_watch::{Entry, Hint, Watcher};
use tokio::sync::mpsc::UnboundedSender;

/// Names that are tools' leftovers, not rules: temp files of the engine and of editors, lock files,
/// swap files, Vim's probe file (`4913`), sync conflicts.
fn ignored_name(name: &str) -> bool {
    name.contains(".rules-tmp.")
        || name.ends_with('~')
        || name.ends_with(".swp")
        || name.ends_with(".swo")
        || name.ends_with(".swn")
        || name.ends_with(".swx")
        || name.starts_with(".#")
        || name.contains(".sync-conflict-")
        || name.contains(" (conflicted copy)")
        || (name.starts_with('.') && name.ends_with(".lock"))
        || (!name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()))
}

fn is_markdown(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

/// Is a change at this path, given relative to the vault, worth a rescan?
///
/// `entry` says what is at the path; it is asked only when the place of the path could matter at
/// all (it costs a system call, and a vault that is a git repository makes hundreds of events under
/// `.git`).
#[must_use]
pub fn relevant(relative: &Path, entry: impl FnOnce() -> Entry) -> bool {
    let Some(parts) = relative
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect::<Option<Vec<&str>>>()
    else {
        // A name that is not text: the periodic scan will see what it is.
        return false;
    };
    match parts.as_slice() {
        [] => false,
        // The settings of the vault, which say which rule sets are loaded.
        [".rules.json"] => true,
        // The folder of the rule sets, a rule set that came or went (a folder only).
        ["rulesets"] => entry() != Entry::File,
        ["rulesets", name] => {
            !name.starts_with('.') && !ignored_name(name) && entry() != Entry::File
        }
        // Inside a rule set: a markdown file, or a folder (a folder is moved, renamed and removed as
        // a whole, and the system then reports the folder and not the files in it).
        ["rulesets", _, .., last] => {
            if ignored_name(last) || last.starts_with('.') {
                return false;
            }
            is_markdown(last) || entry() != Entry::File
        }
        _ => false,
    }
}

/// [`relevant`] as the kit's filter.
fn filter(relative: &Path, entry: &dyn Fn() -> Entry) -> bool {
    relevant(relative, entry)
}

/// Watch the vault and send a hint for every change that matters.
///
/// # Errors
///
/// The reason, as text: the watcher could not be created or could not watch the folder.
pub fn start(vault: &Path, hints: UnboundedSender<Hint>) -> Result<Watcher, String> {
    appkit_hub::fs_watch::start(vault, hints, filter).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(p: &str) -> bool {
        relevant(Path::new(p), || Entry::File)
    }

    fn folder(p: &str) -> bool {
        relevant(Path::new(p), || Entry::Folder)
    }

    fn gone(p: &str) -> bool {
        relevant(Path::new(p), || Entry::Gone)
    }

    #[test]
    fn the_files_of_a_rule_set_matter() {
        for yes in [
            "rulesets/lua/regeln/LUA.md",
            "rulesets/lua/_rules/LUA/LUA-01.md",
            "rulesets/lua/_layouts/regeln/LUA.md",
            "rulesets/lua/_retired/GO/GO-41.md",
            "rulesets/lua/regeln/UPPER.MD",
            ".rules.json",
        ] {
            assert!(file(yes), "{yes}");
        }
    }

    #[test]
    fn a_rule_set_that_came_or_went_matters() {
        for yes in ["rulesets", "rulesets/new-set", "rulesets/lua/_rules/NEW"] {
            assert!(folder(yes), "{yes}");
            assert!(gone(yes), "{yes}");
        }
    }

    #[test]
    fn what_is_not_a_rule_does_not_matter() {
        for no in [
            "README.md",
            ".git/index",
            ".git/refs/heads/main",
            ".github/workflows/ci.yml",
            "rulesets/lua/notes.txt",
            "rulesets/lua/regeln/LUA.md.swp",
            "rulesets/lua/regeln/LUA.md~",
            "rulesets/lua/regeln/.#LUA.md",
            "rulesets/lua/regeln/4913",
            "rulesets/lua/regeln/LUA.rules-tmp.1.md",
            "rulesets/lua/regeln/.LUA.md.lock",
            "rulesets/lua/regeln/LUA (conflicted copy).md",
            "rulesets/lua/.git/HEAD",
            "elsewhere/lua/regeln/LUA.md",
        ] {
            assert!(!file(no), "{no}");
        }
        // A file in the folder of the rule sets is not a rule set; a hidden folder is not either.
        assert!(!file("rulesets"));
        assert!(!folder("rulesets/.git"));
        assert!(!folder(".git"));
        assert!(!folder("elsewhere"));
    }

    #[test]
    fn a_removed_non_markdown_name_might_have_been_a_folder() {
        assert!(gone("rulesets/lua/regeln/sub"));
        assert!(!gone("rulesets/lua/regeln/LUA.md.swp"));
    }
}
