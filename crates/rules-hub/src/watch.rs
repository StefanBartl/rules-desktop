//! What changed in the vault, as a hint: the watcher is the kit's, the rule of which paths matter
//! is the rules hub's own and stays here, so that the kit cannot be made to watch less.
//!
//! The engine reads the rule sets under `<vault>/rulesets/` and the settings in `.rules.json`
//! files. What counts is a Markdown file (`.md` in any case), a `.rules.json`, and a folder (a rule
//! set or a folder of rules that is made, renamed or removed as a whole). Nothing under a folder
//! whose name starts with a dot counts (`.git`, the engine's `.rules-ops`), and neither do the
//! leftovers of editors and sync tools.
//!
//! A limit of the watcher: `<vault>/.rules.json` may name rule sets outside the vault. Those are
//! not watched; a change in them is seen by the timer of the refresher (only while an event
//! stream is open) and by the rescan the page asks for when it gets the focus back.

use std::path::{Path, PathBuf};

use notify::Event;
use tokio::sync::mpsc::UnboundedSender;

pub use appkit_hub::fs_watch::{Entry, Hint, Watcher};

/// The settings file of a vault, a project or a rule set.
const SETTINGS_FILE: &str = ".rules.json";

/// Names that are tools' leftovers, not rules: editor backups and swap files, Vim's probe file
/// (`4913`), Emacs lock files, sync conflicts.
fn ignored_name(name: &str) -> bool {
    name.ends_with('~')
        || name.ends_with(".swp")
        || name.ends_with(".swo")
        || name.ends_with(".swn")
        || name.ends_with(".swx")
        || name.starts_with(".#")
        || name.contains(".sync-conflict-")
        || name.contains(" (conflicted copy)")
        || (!name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()))
}

fn is_markdown(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

/// Is a change at this path, given relative to the vault, worth a rescan?
///
/// `entry` says what is at the path; it is asked only when the name alone does not decide (it
/// costs a system call, and a vault that is a git repository makes hundreds of events under
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
    let Some((last, folders)) = parts.split_last() else {
        return false;
    };
    // Under a hidden folder: a tool's state, never a rule set.
    if folders.iter().any(|f| f.starts_with('.')) {
        return false;
    }
    if *last == SETTINGS_FILE {
        return true;
    }
    if last.starts_with('.') || ignored_name(last) {
        return false;
    }
    is_markdown(last) || entry() != Entry::File
}

/// [`relevant`] as the kit's filter.
fn filter(relative: &Path, entry: &dyn Fn() -> Entry) -> bool {
    relevant(relative, entry)
}

/// What an event of the watcher asks of the refresher, if anything.
#[must_use]
pub fn hint_for(event: &Event, bases: &[PathBuf]) -> Option<Hint> {
    appkit_hub::fs_watch::hint_for(event, bases, filter)
}

/// Watch the vault and send a hint for every change that matters.
///
/// # Errors
///
/// The watcher could not be created or could not watch the folder.
pub fn start(vault: &Path, hints: UnboundedSender<Hint>) -> notify::Result<Watcher> {
    appkit_hub::fs_watch::start(vault, hints, filter)
}

#[cfg(test)]
mod tests {
    use notify::event::RenameMode;
    use notify::event::{AccessKind, CreateKind, DataChange, Flag, ModifyKind, RemoveKind};
    use notify::EventKind;

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
    fn rule_files_and_settings_matter() {
        for yes in [
            "rulesets/lua/DEP.md",
            "rulesets/lua/_rules/ERR/ERR-100.md",
            "rulesets/lua/_retired/ERR/ERR-007.md",
            "rulesets/lua/UPPER.MD",
            "rulesets/lua/Mixed.Md",
            ".rules.json",
            "rulesets/lua/.rules.json",
        ] {
            assert!(file(yes), "{yes}");
            assert!(gone(yes), "{yes}");
        }
    }

    #[test]
    fn a_folder_made_renamed_or_removed_as_a_whole_matters() {
        for path in [
            "rulesets",
            "rulesets/new-set",
            "rulesets/lua/_rules/NEW",
            "rulesets/lua/_retired",
        ] {
            assert!(folder(path), "{path}");
            assert!(gone(path), "{path}");
        }
        // A file that is not Markdown is no rule; one that was removed might have been a folder.
        assert!(!file("rulesets/lua/notes.txt"));
        assert!(gone("rulesets/lua/notes.txt"));
    }

    #[test]
    fn nothing_under_a_hidden_folder_matters() {
        for no in [
            ".git",
            ".git/HEAD",
            ".git/index.lock",
            ".rules-ops/journal.md",
            "rulesets/lua/.rules-ops/op-1.md",
            "rulesets/.hidden/DEP.md",
            ".obsidian/workspace.json",
            ".rules.json/x.md",
        ] {
            assert!(!file(no), "{no}");
            assert!(!folder(no), "{no}");
            assert!(!gone(no), "{no}");
        }
    }

    #[test]
    fn what_an_editor_or_a_sync_tool_leaves_does_not_matter() {
        for no in [
            "rulesets/lua/DEP.md~",
            "rulesets/lua/.DEP.md.swp",
            "rulesets/lua/DEP.md.swp",
            "rulesets/lua/.DEP.md.swo",
            "rulesets/lua/.#DEP.md",
            "rulesets/lua/4913",
            "rulesets/lua/DEP.sync-conflict-20261017.md",
            "rulesets/lua/DEP (conflicted copy).md",
            "rulesets/lua/.DS_Store",
        ] {
            assert!(!file(no), "{no}");
            // Even when they are gone again: the name says what they were.
            assert!(!gone(no), "{no}");
        }
    }

    #[test]
    fn the_file_system_is_asked_only_where_the_name_does_not_decide() {
        for path in [
            ".git",
            ".git/objects/ab/cdef",
            "rulesets/lua/DEP.md",
            "rulesets/lua/DEP.md~",
            "rulesets/lua/4913",
            ".rules.json",
            "",
        ] {
            let _ = relevant(Path::new(path), || panic!("asked for {path}"));
        }
    }

    fn bases() -> Vec<PathBuf> {
        vec![PathBuf::from("vault")]
    }

    fn event(kind: EventKind, path: &str) -> Event {
        Event::new(kind).add_path(PathBuf::from("vault").join(path))
    }

    #[test]
    fn a_dropped_event_is_a_lost_one() {
        let overflow = Event::new(EventKind::Other).set_flag(Flag::Rescan);
        assert_eq!(hint_for(&overflow, &bases()), Some(Hint::Lost));
    }

    #[test]
    fn events_are_told_apart_by_kind_and_path() {
        let base = &bases();
        let rule = "rulesets/lua/DEP.md";
        assert_eq!(
            hint_for(&event(EventKind::Create(CreateKind::File), rule), base),
            Some(Hint::Changed)
        );
        assert_eq!(
            hint_for(
                &event(EventKind::Modify(ModifyKind::Data(DataChange::Any)), rule),
                base
            ),
            Some(Hint::Changed)
        );
        assert_eq!(
            hint_for(&event(EventKind::Remove(RemoveKind::File), rule), base),
            Some(Hint::Changed)
        );
        // A read is not a change.
        assert_eq!(
            hint_for(&event(EventKind::Access(AccessKind::Any), rule), base),
            None
        );
        assert_eq!(
            hint_for(
                &event(EventKind::Create(CreateKind::Any), ".git/HEAD"),
                base
            ),
            None
        );
        // The old name of a rule set that was renamed is gone, and it is told by that name alone.
        let old = event(
            EventKind::Modify(ModifyKind::Name(RenameMode::From)),
            "rulesets/old-name",
        );
        assert_eq!(hint_for(&old, base), Some(Hint::Changed));
    }
}
