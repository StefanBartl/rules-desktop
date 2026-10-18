//! The settings of this app in `config.json`, beside the keys of the kit (vault, nvim, engine_dir, port).
//!
//! `project` is the folder whose `.rules-waivers.json` the waivers are kept in. It is a statement of the
//! person about this device, never of the page: the hub puts it into every waiver operation itself and refuses a
//! `project` that comes from outside (a page with a session could otherwise make the engine write that file into any
//! folder of the machine). `RULES_DESKTOP_PROJECT` overrides the file.

use std::path::{Path, PathBuf};

use appkit_hub::config::AppSettings;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RulesSettings {
    /// The project whose waivers are kept and changed here; none: waivers are not available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<PathBuf>,
}

/// Is this a path the hub may hand to the engine as the project? Whole (absolute), no `..`, no control character.
///
/// # Errors
///
/// The reason, without the path.
pub fn check_project(path: &Path) -> Result<(), &'static str> {
    let text = path.to_string_lossy();
    if text.is_empty() || !path.is_absolute() {
        return Err("project must be a whole path");
    }
    if text.chars().any(char::is_control) {
        return Err("project holds a control character");
    }
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("project must not climb with ..");
    }
    Ok(())
}

impl AppSettings for RulesSettings {
    fn validate(&self) -> Result<(), String> {
        match &self.project {
            Some(path) => check_project(path).map_err(str::to_owned),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_is_a_whole_path_without_climbing_or_control_characters() {
        let whole = std::env::temp_dir();
        assert!(check_project(&whole).is_ok());
        for bad in [
            PathBuf::new(),
            PathBuf::from("relative/dir"),
            whole.join(".."),
            PathBuf::from(format!("{}\u{7}x", whole.display())),
        ] {
            assert!(check_project(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_settings_validate_their_project_and_refuse_a_key_they_do_not_know() {
        let ok = RulesSettings {
            project: Some(std::env::temp_dir()),
        };
        assert!(ok.validate().is_ok());
        assert!(RulesSettings::default().validate().is_ok());
        let bad = RulesSettings {
            project: Some(PathBuf::from("nope")),
        };
        assert!(bad.validate().is_err());
        assert!(serde_json::from_str::<RulesSettings>(r#"{"projects":[]}"#).is_err());
    }
}
