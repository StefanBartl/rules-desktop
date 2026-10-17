//! Where the file of a rule is. The `source` of a rule is a name (`<ruleset name>/<path>`, with the folder
//! above on a clash and `#n` after that) and not a place; the one that knows the place is the engine, and the
//! naming rule is not rebuilt here. rules.nvim answers `scripts/rules.lua locate <id>` with two plain lines, the
//! file and the ruleset it was loaded from. That is not a document of `rules-export/1` (which holds no path of
//! this machine), so it is a question of its own, asked of a process of its own, with the environment the engine
//! gets and nothing else.

use std::path::PathBuf;
use std::time::Duration;

use appkit_hub::engine::{child_env, CliEngineConfig, EngineVars};
use appkit_hub::BoxFuture;

/// Where a rule's file is and which ruleset holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub file: PathBuf,
    /// `None`: the engine knows no ruleset that holds the file.
    pub root: Option<PathBuf>,
}

/// Asks where the file of a rule is.
pub trait Locate: Send + Sync {
    /// # Errors
    ///
    /// A short word for the page: no path, no text of the engine.
    fn locate<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Located, &'static str>>;
}

/// The engine as a process: one `rules.lua locate` for each question.
pub struct ProcessLocator {
    cfg: CliEngineConfig,
}

/// The longest the question may take.
const TIMEOUT: Duration = Duration::from_secs(30);

impl ProcessLocator {
    #[must_use]
    pub fn new(cfg: CliEngineConfig) -> Self {
        Self { cfg }
    }

    async fn ask(&self, id: &str) -> Result<Located, &'static str> {
        let vars = EngineVars {
            dialect: self.cfg.dialect,
            vault: &self.cfg.vault,
            lib_dir: self.cfg.lib_dir.as_deref(),
            extra_areas: &[],
        };
        let mut command = tokio::process::Command::new(&self.cfg.nvim);
        command
            .args(["--headless", "-u", "NONE", "-i", "NONE", "-n", "-l"])
            .arg(&self.cfg.script)
            .arg("locate")
            .arg(id)
            .env_clear()
            .envs(child_env(&self.cfg.env, Some(&vars)))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let child = command
            .spawn()
            .map_err(|_| "the engine could not be started")?;
        let output = tokio::time::timeout(TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| "the engine did not answer in time")?
            .map_err(|_| "the engine could not be read")?;
        match output.status.code() {
            Some(0) => parse(&String::from_utf8_lossy(&output.stdout)),
            // The engine does not hold the id: the page was told by the snapshot it does, so the vault changed.
            Some(1) => Err("the engine does not hold this rule"),
            _ => Err("the engine could not tell where the file is"),
        }
    }
}

/// The two lines of the answer: the file, then the ruleset (empty: none).
fn parse(text: &str) -> Result<Located, &'static str> {
    let mut lines = text.lines();
    let file = lines
        .next()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .ok_or("the engine named no file")?;
    let root = lines.next().map(str::trim_end).filter(|l| !l.is_empty());
    if lines.next().is_some() {
        return Err("the engine answered more than two lines");
    }
    Ok(Located {
        file: PathBuf::from(file),
        root: root.map(PathBuf::from),
    })
}

impl Locate for ProcessLocator {
    fn locate<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Located, &'static str>> {
        Box::pin(self.ask(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_lines_are_the_file_and_the_ruleset() {
        let got = parse("/v/a/b.md\n/v/a\n").unwrap();
        assert_eq!(got.file, PathBuf::from("/v/a/b.md"));
        assert_eq!(got.root, Some(PathBuf::from("/v/a")));
        // A Windows engine may end its lines with CR LF.
        let got = parse("C:/v/a/b.md\r\nC:/v/a\r\n").unwrap();
        assert_eq!(got.file, PathBuf::from("C:/v/a/b.md"));
        assert_eq!(got.root, Some(PathBuf::from("C:/v/a")));
        // No ruleset named: the file is there, and nothing says where it was loaded from.
        assert_eq!(parse("/v/a/b.md\n\n").unwrap().root, None);
        assert_eq!(parse("/v/a/b.md\n").unwrap().root, None);
    }

    #[test]
    fn anything_else_is_not_an_answer() {
        for bad in ["", "\n", "\n/v\n", "/a\n/b\n/c\n"] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }
}
