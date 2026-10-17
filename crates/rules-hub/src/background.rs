//! The hub as a background program (`start`, `stop`, `autostart`, `install`, `uninstall`): the work
//! is the kit's, this module states what is the rules hub's own.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use appkit_hub::background::Background;
pub use appkit_hub::background::{
    run_command, start_from, Autostart, InstallOptions, UninstallOptions,
};

use appkit_hub::env::Env;

/// The files of the rules hub in the config folder besides those every hub has.
const EXTRA_FILES: [&str; 1] = ["sources.json"];

/// What the background commands know about this app.
pub(crate) const APP: Background<'static> = Background {
    login: crate::LOGIN,
    extra_files: &EXTRA_FILES,
    uninstall_script: "scripts/uninstall",
};

/// Start a hub in the background unless one runs. Returns what to print.
///
/// # Errors
///
/// A text for the person at the terminal.
pub async fn start() -> Result<String, String> {
    appkit_hub::background::start(&APP).await
}

/// Stop the hub that runs (the one `hub.json` names and that answers as it).
///
/// # Errors
///
/// A text for the person at the terminal.
pub async fn stop() -> Result<String, String> {
    appkit_hub::background::stop(&APP).await
}

/// Start the hub with the login of the user.
///
/// # Errors
///
/// A text for the person at the terminal.
pub fn autostart(what: Autostart) -> Result<String, String> {
    appkit_hub::background::autostart(&APP, what)
}

/// Where `install` puts this program.
///
/// # Errors
///
/// The reason there is no install folder.
pub fn installed_program(env: &Env) -> Result<PathBuf, String> {
    appkit_hub::background::installed_program(&APP, env)
}

/// Copy this program to a fixed place and start the hub from there again when one was running.
///
/// # Errors
///
/// A text for the person at the terminal.
pub async fn install(opts: InstallOptions) -> Result<String, String> {
    appkit_hub::background::install(&APP, opts).await
}

/// Take the integration of the program with the system away.
pub fn uninstall(
    opts: UninstallOptions,
) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
    appkit_hub::background::uninstall(&APP, opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `uninstall --purge-settings` removes these files and nothing else.
    #[test]
    fn the_files_of_the_rules_hub_besides_the_kits_are_named() {
        assert_eq!(APP.extra_files, ["sources.json"]);
        assert_eq!(APP.login.summary, "the rule set");
        assert!(crate::IDENTITY.program_file().starts_with("rules-hub"));
    }

    #[test]
    fn the_desktop_link_is_the_hubs_own() {
        let make = appkit_hub::background::link_script(
            crate::IDENTITY.title,
            crate::IDENTITY.app,
            r"C:\x\rules-hub.exe",
        );
        assert!(make.contains(r"\Rules.lnk"), "{make}");
        assert!(
            make.contains(r#"link.TargetPath = "C:\x\rules-hub.exe""#),
            "{make}"
        );
        assert!(
            make.contains(r#"link.Arguments = "start --open""#),
            "{make}"
        );
        assert!(!make.contains("Tasks") && !make.contains("tasks"), "{make}");
    }
}
