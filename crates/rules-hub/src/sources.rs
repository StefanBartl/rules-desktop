//! Data classes of the rule sets, per device.
//!
//! `sources.json` lives in the app's config folder (`%APPDATA%/rules-desktop` and the like), never
//! in the vault: the vault is synced and changed by pull requests, and a release written there could
//! be set by a third party. **Fail closed:** a file of a rule set without an entry is
//! `unclassified`, and nothing unclassified or private is released for anything that leaves the
//! hub. Showing it to the user on this machine is always allowed and is not a use.
//!
//! ```json
//! { "schema": 1,
//!   "default": "internal",
//!   "sources": { "lua": { "class": "internal" },
//!                "wkdbook-Lua/Checklists": { "class": "private" } } }
//! ```
//!
//! A key is the name of a rule set as the engine writes it at the start of `source`
//! (`<rule set name>/<path inside it>`). That name depends on what is loaded with it
//! (rules.nvim `lua/rules/contract.lua`, `root_names`): it is the last folder of the rule set
//! (`Checklists`), the folder above is added only when two rule sets would share a name
//! (`wkdbook-Lua/Checklists`), and a name that is still taken gets a number in the order of the
//! configuration (`Checklists#2`). So a file is classified like this:
//!
//! - every key `K` with `source == K` or `source` starting with `K/` counts, and of several the
//!   **strictest** class wins (`private` over `internal` over `public`): a key of another rule set
//!   that happens to be a prefix of the name cannot release it;
//! - no such key: the `default` of the file, the owner's explicit choice; without one,
//!   `unclassified`;
//! - a key may also name the same rule set under its other name (`Checklists` before a second
//!   `Checklists` is loaded, `wkdbook-Lua/Checklists` after, and back): a key whose last part is
//!   the first part of `source`, and a key that `source` without its first part is or starts
//!   with (`Checklists` or the folder `Checklists/_rules` for `wkdbook-Lua/Checklists/_rules/...`),
//!   may only make the class **stricter**, never looser. Without a default an explicit `private`
//!   of such a key still counts, a `public` or `internal` one does not;
//! - a `source` whose first or second part holds a `#` is always `unclassified`, even with a
//!   default: `#n` follows the order of the configuration and can move from one rule set to
//!   another. For the same reason a key with `#` is refused, and the hub does not start;
//! - a `source` that is not of the engine's shape (empty first part, a `..` part, the engine's
//!   `<external>/` for a file outside every rule set) is `unclassified`.
//!
//! Names are compared without regard to ASCII letter case, as the engine compares them when it
//! tells rule sets apart; two keys that differ only in case are refused.

use std::collections::HashSet;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};

/// The class of a rule set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Class {
    /// May leave the hub, e.g. into a public report.
    Public,
    /// Mine and not secret: may go to another panel of the same user, to a cache of the client.
    Internal,
    /// Never leaves the hub: not to another panel, not to a runner, not into a cache of the client.
    Private,
}

/// The class a file is shown with, from the loosest to the strictest: the order is what "the
/// strictest wins" compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Shown {
    Public,
    Internal,
    Private,
    /// Nobody said: released for nothing.
    Unclassified,
}

impl From<Class> for Shown {
    fn from(class: Class) -> Self {
        match class {
            Class::Public => Self::Public,
            Class::Internal => Self::Internal,
            Class::Private => Self::Private,
        }
    }
}

impl Shown {
    /// May a file of this class leave the hub (another panel, a cache of the client)? Only what
    /// somebody released: `public` or `internal`.
    #[must_use]
    pub fn released(self) -> bool {
        matches!(self, Self::Public | Self::Internal)
    }
}

/// One entry of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    class: Class,
}

/// The entries in the order of the file, a repeated key included. A JSON object may repeat a key,
/// and a map would keep the last one: `{"a": private, "a": public}` would release `a`. So every
/// entry is kept and [`SourcePolicy::new`] refuses the repetition, as it refuses a key that
/// differs only in letter case.
struct Keys(Vec<(String, Class)>);

impl<'de> Deserialize<'de> for Keys {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KeysVisitor;

        impl<'de> Visitor<'de> for KeysVisitor {
            type Value = Keys;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object of rule set names")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Keys, A::Error> {
                let mut out = Vec::new();
                while let Some((key, entry)) = map.next_entry::<String, Entry>()? {
                    // One more than the most is enough for `new` to refuse the file.
                    if out.len() > MAX_KEYS {
                        return Err(de::Error::custom(format!("more than {MAX_KEYS} keys")));
                    }
                    out.push((key, entry.class));
                }
                Ok(Keys(out))
            }
        }

        deserializer.deserialize_map(KeysVisitor)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema: u32,
    /// The class of a file no key names (the owner's explicit choice).
    #[serde(default)]
    default: Option<Class>,
    sources: Keys,
}

/// The schema of `sources.json` this build reads.
pub const SCHEMA: u32 = 1;

/// The most keys a file may name.
pub const MAX_KEYS: usize = 1000;
/// The largest file that is read.
pub const MAX_FILE_BYTES: u64 = 256 * 1024;
/// The longest key, in bytes.
const MAX_KEY_BYTES: usize = 200;

/// What `source` the engine writes for a file outside every rule set.
const EXTERNAL: &str = "<external>";

/// What is known about the rule sets of this device. The default policy classifies nothing.
#[derive(Debug, Clone, Default)]
pub struct SourcePolicy {
    /// Sorted by key.
    keys: Vec<(String, Class)>,
    default: Option<Class>,
}

/// Why `sources.json` is not usable. The hub does not start with a file it cannot use: a typo must
/// not turn into "everything unclassified", or into a release, unseen.
#[derive(Debug)]
pub enum SourcesError {
    Unreadable(PathBuf, std::io::Error),
    Invalid(PathBuf, String),
}

impl fmt::Display for SourcesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(p, e) => write!(f, "cannot read {}: {e}", p.display()),
            Self::Invalid(p, e) => write!(f, "invalid {}: {e}", p.display()),
        }
    }
}

impl std::error::Error for SourcesError {}

/// The path of the file in a config folder.
#[must_use]
pub fn path(dir: &Path) -> PathBuf {
    dir.join("sources.json")
}

/// Is `key` the name of a rule set as the engine writes it: one or two parts of
/// `[A-Za-z0-9._-]`, joined by `/`, no part that starts with a dot, no `..`, and no `#`?
///
/// # Errors
///
/// Why it is not.
pub fn valid_key(key: &str) -> Result<(), &'static str> {
    if key.contains('#') {
        return Err(
            "a key with # names a rule set by its place in the configuration, \
                    which can move to another rule set; name the folder above instead",
        );
    }
    if key.is_empty() || key.len() > MAX_KEY_BYTES {
        return Err("a key is 1 to 200 bytes");
    }
    if key.contains("..") {
        return Err("a key holds no ..");
    }
    let parts: Vec<&str> = key.split('/').collect();
    if parts.len() > 2 {
        return Err("a key is a name or <folder>/<name>, with at most one /");
    }
    for part in parts {
        if part.is_empty() {
            return Err("a key has no empty part");
        }
        if part.starts_with('.') {
            return Err("a part of a key does not start with a dot");
        }
        if !part
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err("a key is made of letters, digits, '.', '_' and '-'");
        }
    }
    Ok(())
}

/// How one file is classified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict<'a> {
    pub shown: Shown,
    /// The key that decided the class; `None` when it is the default or nothing.
    pub key: Option<&'a str>,
    /// Every key that counted for the file.
    pub matched: Vec<&'a str>,
}

impl<'a> Verdict<'a> {
    fn unclassified() -> Self {
        Self {
            shown: Shown::Unclassified,
            key: None,
            matched: Vec::new(),
        }
    }
}

/// `text` is `prefix`, or starts with `prefix` and a `/`, without regard to ASCII case.
fn is_under(text: &str, prefix: &str) -> bool {
    let (t, p) = (text.as_bytes(), prefix.as_bytes());
    t.len() >= p.len()
        && t[..p.len()].eq_ignore_ascii_case(p)
        && (t.len() == p.len() || t[p.len()] == b'/')
}

/// Does `new` make a file stricter than `old` without releasing it? An explicit `private` replaces
/// `unclassified` (both are released for nothing, but the owner said it); `public` or `internal`
/// never does, since that would release what nobody released.
fn stricter(new: Shown, old: Shown) -> bool {
    match old {
        Shown::Unclassified => new == Shown::Private,
        _ => new > old,
    }
}

impl SourcePolicy {
    /// Build a policy from keys, checking every one.
    ///
    /// # Errors
    ///
    /// The reason for the first key that is not acceptable.
    pub fn new(keys: impl IntoIterator<Item = (String, Class)>) -> Result<Self, String> {
        let mut out: Vec<(String, Class)> = Vec::new();
        let mut seen = HashSet::new();
        for (key, class) in keys {
            if out.len() >= MAX_KEYS {
                return Err(format!("more than {MAX_KEYS} keys"));
            }
            valid_key(&key).map_err(|why| format!("key {key:?}: {why}"))?;
            if !seen.insert(key.to_ascii_lowercase()) {
                return Err(format!(
                    "key {key:?} is given twice (letter case does not tell keys apart)"
                ));
            }
            out.push((key, class));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Self {
            keys: out,
            default: None,
        })
    }

    /// The class of a file no key names. `None` keeps such a file unclassified.
    #[must_use]
    pub fn with_default(mut self, default: Option<Class>) -> Self {
        self.default = default;
        self
    }

    /// Load `sources.json` from the config folder. No file is a policy in which nothing is
    /// classified.
    ///
    /// # Errors
    ///
    /// [`SourcesError`] for a file that exists and cannot be used: the hub does not start then.
    pub fn load(dir: &Path) -> Result<Self, SourcesError> {
        let file_path = path(dir);
        let file = match std::fs::File::open(&file_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(SourcesError::Unreadable(file_path, e)),
        };
        let mut text = String::new();
        if let Err(e) = file.take(MAX_FILE_BYTES + 1).read_to_string(&mut text) {
            return Err(SourcesError::Unreadable(file_path, e));
        }
        if text.len() as u64 > MAX_FILE_BYTES {
            return Err(SourcesError::Invalid(
                file_path,
                format!("larger than {} KiB", MAX_FILE_BYTES / 1024),
            ));
        }
        let parsed: File = serde_json::from_str(&text)
            .map_err(|e| SourcesError::Invalid(file_path.clone(), e.to_string()))?;
        if parsed.schema != SCHEMA {
            return Err(SourcesError::Invalid(
                file_path,
                format!("schema {} is not {SCHEMA}", parsed.schema),
            ));
        }
        Self::new(parsed.sources.0)
            .map(|p| p.with_default(parsed.default))
            .map_err(|why| SourcesError::Invalid(file_path, why))
    }

    /// The keys of the file with their classes, sorted.
    pub fn keys(&self) -> impl Iterator<Item = (&str, Class)> {
        self.keys.iter().map(|(k, c)| (k.as_str(), *c))
    }

    /// The class of a file without a key.
    #[must_use]
    pub fn default_class(&self) -> Option<Class> {
        self.default
    }

    /// How many keys the file has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The class a file is shown with, by its `source` (`<rule set name>/<path inside it>`).
    #[must_use]
    pub fn shown(&self, source: &str) -> Shown {
        self.classify(source).shown
    }

    /// How a file is classified, by its `source`; see the module for the rules.
    #[must_use]
    pub fn classify(&self, source: &str) -> Verdict<'_> {
        let parts: Vec<&str> = source.split('/').collect();
        let first = parts[0];
        let second = parts.get(1).copied();
        if first.is_empty()
            || first.eq_ignore_ascii_case(EXTERNAL)
            || parts.contains(&"..")
            || first.contains('#')
            || second.is_some_and(|s| s.contains('#'))
        {
            return Verdict::unclassified();
        }
        let mut verdict = Verdict {
            shown: Shown::Unclassified,
            key: None,
            matched: Vec::new(),
        };
        // The keys that name the file's rule set or a folder above it: the strictest wins.
        let mut best: Option<(Shown, &str)> = None;
        for (key, class) in &self.keys {
            if is_under(source, key) {
                verdict.matched.push(key);
                let shown = Shown::from(*class);
                if best.is_none_or(|(s, k)| shown > s || (shown == s && key.len() > k.len())) {
                    best = Some((shown, key));
                }
            }
        }
        match best {
            Some((shown, key)) => {
                verdict.shown = shown;
                verdict.key = Some(key);
            }
            None => {
                verdict.shown = self.default.map_or(Shown::Unclassified, Shown::from);
            }
        }
        // The keys that may name the same rule set under its other name: only ever stricter.
        // `rest` is the source without its first part, as it reads once the folder above is no
        // longer added to the name.
        let rest = source.split_once('/').map(|(_, r)| r);
        for (key, class) in &self.keys {
            if verdict.matched.contains(&key.as_str()) {
                continue;
            }
            let last = key.rsplit('/').next().unwrap_or(key);
            // `wkdbook-Lua/Checklists` for `Checklists/...`; `Checklists` for
            // `wkdbook-Lua/Checklists/...`, and a folder inside it, `Checklists/_rules`, for
            // `wkdbook-Lua/Checklists/_rules/...`.
            let alias = last.eq_ignore_ascii_case(first) || rest.is_some_and(|r| is_under(r, key));
            if !alias {
                continue;
            }
            verdict.matched.push(key);
            let shown = Shown::from(*class);
            if stricter(shown, verdict.shown) {
                verdict.shown = shown;
                verdict.key = Some(key);
            }
        }
        verdict
    }

    /// The document of `GET /api/rules/sources` for the files of a snapshot: each file with the
    /// class it is shown with and the key that decided it, each key with the number of files it
    /// counted for, and the paths that are unclassified or private.
    #[must_use]
    pub fn document(&self, files: &[SourceFile]) -> Value {
        let mut out = Vec::with_capacity(files.len());
        let mut unclassified = Vec::new();
        let mut private = Vec::new();
        let mut counts = vec![0_u64; self.keys.len()];
        for file in files {
            let verdict = self.classify(&file.path);
            match verdict.shown {
                Shown::Unclassified => unclassified.push(file.path.as_str()),
                Shown::Private => private.push(file.path.as_str()),
                Shown::Public | Shown::Internal => {}
            }
            // The keys are sorted and unique: each matched one is found, not searched for.
            for key in &verdict.matched {
                if let Ok(i) = self.keys.binary_search_by(|(k, _)| k.as_str().cmp(key)) {
                    counts[i] += 1;
                }
            }
            out.push(json!({
                "path": file.path,
                "data_class": verdict.shown,
                "rules": file.rules,
                "key": verdict.key,
            }));
        }
        let keys: Vec<Value> = self
            .keys
            .iter()
            .zip(counts)
            .map(|((key, class), files)| json!({ "key": key, "class": class, "files": files }))
            .collect();
        json!({
            "schema": 1,
            "kind": "hub.sources",
            "files": out,
            "keys": keys,
            "unclassified": unclassified,
            "private": private,
        })
    }
}

/// A file of the snapshot (`sources[]` of `rules.snapshot`): its path and how many rules it holds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub rules: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str, text: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rules-desktop-sources-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(path(&dir), text).unwrap();
        dir
    }

    fn load(name: &str, text: &str) -> Result<SourcePolicy, SourcesError> {
        SourcePolicy::load(&temp(name, text))
    }

    fn policy(keys: &[(&str, Class)], default: Option<Class>) -> SourcePolicy {
        SourcePolicy::new(keys.iter().map(|(k, c)| ((*k).to_owned(), *c)))
            .unwrap()
            .with_default(default)
    }

    use Class::{Internal, Private, Public};

    #[test]
    fn a_missing_file_classifies_nothing() {
        let dir =
            std::env::temp_dir().join(format!("rules-desktop-sources-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let policy = SourcePolicy::load(&dir).unwrap();
        assert!(policy.is_empty());
        assert_eq!(policy.default_class(), None);
        assert_eq!(policy.shown("Checklists/DEP.md"), Shown::Unclassified);
        assert!(!policy.shown("Checklists/DEP.md").released());
    }

    #[test]
    fn the_file_of_the_design_note_loads() {
        let p = load(
            "good",
            r#"{ "schema": 1, "default": "internal",
                 "sources": { "lua": { "class": "internal" },
                              "native": { "class": "public" },
                              "wkdbook-Lua/Checklists": { "class": "private" } } }"#,
        )
        .unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p.default_class(), Some(Internal));
        assert_eq!(p.shown("native/REL.md"), Shown::Public);
        assert_eq!(p.shown("lua/x/y.md"), Shown::Internal);
        assert_eq!(p.shown("wkdbook-Lua/Checklists/DEP.md"), Shown::Private);
        assert_eq!(p.shown("other/DEP.md"), Shown::Internal, "the default");
    }

    #[test]
    fn a_file_that_is_not_right_is_refused() {
        for (name, text) in [
            ("json", "{"),
            ("typo", r#"{"schema":1,"sources":{"a":{"clas":"private"}}}"#),
            (
                "class",
                r#"{"schema":1,"sources":{"a":{"class":"secret"}}}"#,
            ),
            (
                "entry-extra",
                r#"{"schema":1,"sources":{"a":{"class":"public","repo":"a"}}}"#,
            ),
            ("schema", r#"{"schema":2,"sources":{}}"#),
            ("no-schema", r#"{"sources":{}}"#),
            ("no-sources", r#"{"schema":1}"#),
            ("extra", r#"{"schema":1,"sources":{},"more":1}"#),
            ("areas", r#"{"schema":1,"areas":{}}"#),
            (
                "default",
                r#"{"schema":1,"default":"unclassified","sources":{}}"#,
            ),
            (
                "twice",
                r#"{"schema":1,"sources":{"a":{"class":"private"},"a":{"class":"public"}}}"#,
            ),
            (
                "case",
                r#"{"schema":1,"sources":{"Lua":{"class":"private"},"lua":{"class":"public"}}}"#,
            ),
            (
                "field-twice",
                r#"{"schema":1,"default":"private","default":"public","sources":{}}"#,
            ),
            (
                "hash",
                r#"{"schema":1,"sources":{"Checklists#2":{"class":"private"}}}"#,
            ),
            (
                "hash-2",
                r#"{"schema":1,"sources":{"wkdbook-Lua/Checklists#2":{"class":"public"}}}"#,
            ),
            (
                "traversal",
                r#"{"schema":1,"sources":{"../x":{"class":"private"}}}"#,
            ),
            (
                "dots",
                r#"{"schema":1,"sources":{"a..b":{"class":"private"}}}"#,
            ),
            (
                "dot",
                r#"{"schema":1,"sources":{".git":{"class":"private"}}}"#,
            ),
            (
                "dot-2",
                r#"{"schema":1,"sources":{"a/.b":{"class":"private"}}}"#,
            ),
            (
                "three",
                r#"{"schema":1,"sources":{"a/b/c":{"class":"private"}}}"#,
            ),
            (
                "slash",
                r#"{"schema":1,"sources":{"a/":{"class":"private"}}}"#,
            ),
            (
                "lead",
                r#"{"schema":1,"sources":{"/a":{"class":"private"}}}"#,
            ),
            (
                "empty",
                r#"{"schema":1,"sources":{"":{"class":"private"}}}"#,
            ),
            (
                "space",
                r#"{"schema":1,"sources":{"a b":{"class":"private"}}}"#,
            ),
            (
                "backslash",
                r#"{"schema":1,"sources":{"a\\b":{"class":"private"}}}"#,
            ),
            (
                "external",
                r#"{"schema":1,"sources":{"<external>":{"class":"public"}}}"#,
            ),
            ("not-object", r#"{"schema":1,"sources":[]}"#),
        ] {
            let got = load(name, text);
            assert!(
                matches!(got, Err(SourcesError::Invalid(..))),
                "{name}: {got:?}"
            );
        }
    }

    #[test]
    fn a_key_with_a_hash_says_why() {
        let Err(SourcesError::Invalid(_, why)) = load(
            "hash-why",
            r#"{"schema":1,"sources":{"Checklists#2":{"class":"private"}}}"#,
        ) else {
            panic!("a # key loaded");
        };
        assert!(why.contains('#') && why.contains("configuration"), "{why}");
    }

    #[test]
    fn a_huge_file_is_refused() {
        let mut text = String::from(r#"{"schema":1,"sources":{"#);
        for i in 0..20_000 {
            text.push_str(&format!(r#""set{i}":{{"class":"public"}},"#));
        }
        text.push_str(r#""z":{"class":"public"}}}"#);
        assert!(text.len() as u64 > MAX_FILE_BYTES);
        let got = load("huge", &text);
        assert!(matches!(got, Err(SourcesError::Invalid(..))), "{got:?}");
    }

    #[test]
    fn more_than_the_most_keys_are_refused() {
        let mut text = String::from(r#"{"schema":1,"sources":{"#);
        let keys: Vec<String> = (0..=MAX_KEYS)
            .map(|i| format!(r#""s{i}":{{"class":"public"}}"#))
            .collect();
        text.push_str(&keys.join(","));
        text.push_str("}}");
        assert!(text.len() as u64 <= MAX_FILE_BYTES);
        let got = load("many", &text);
        assert!(matches!(got, Err(SourcesError::Invalid(..))), "{got:?}");
        let fine: Vec<(String, Class)> = (0..MAX_KEYS).map(|i| (format!("s{i}"), Public)).collect();
        assert_eq!(SourcePolicy::new(fine).unwrap().len(), MAX_KEYS);
    }

    #[test]
    fn of_several_keys_the_strictest_wins() {
        // `wkdbook-Lua` is another, public rule set; the private `Checklists` is called
        // `wkdbook-Lua/Checklists` once a second `Checklists` is loaded. The prefix key must not
        // release it, in whichever order the keys are written.
        for keys in [
            [("wkdbook-Lua", Public), ("wkdbook-Lua/Checklists", Private)],
            [("wkdbook-Lua/Checklists", Private), ("wkdbook-Lua", Public)],
        ] {
            let p = policy(&keys, None);
            let v = p.classify("wkdbook-Lua/Checklists/DEP.md");
            assert_eq!(v.shown, Shown::Private, "{keys:?}");
            assert_eq!(v.key, Some("wkdbook-Lua/Checklists"));
            assert_eq!(v.matched.len(), 2);
            // The public rule set itself stays public.
            assert_eq!(p.shown("wkdbook-Lua/README.md"), Shown::Public);
        }
        // A longer key does not win by its length: the strictest does.
        let p = policy(
            &[("wkdbook-Lua", Private), ("wkdbook-Lua/Checklists", Public)],
            None,
        );
        let v = p.classify("wkdbook-Lua/Checklists/DEP.md");
        assert_eq!(v.shown, Shown::Private);
        assert_eq!(v.key, Some("wkdbook-Lua"));
        let p = policy(
            &[
                ("wkdbook-Lua", Internal),
                ("wkdbook-Lua/Checklists", Public),
            ],
            Some(Private),
        );
        assert_eq!(p.shown("wkdbook-Lua/Checklists/DEP.md"), Shown::Internal);
    }

    #[test]
    fn a_key_counts_only_for_a_whole_name() {
        let p = policy(&[("lua", Public)], None);
        assert_eq!(p.shown("lua/DEP.md"), Shown::Public);
        assert_eq!(p.shown("lua"), Shown::Public, "a rule set that is a file");
        assert_eq!(p.shown("lua-private/DEP.md"), Shown::Unclassified);
        assert_eq!(p.shown("luax"), Shown::Unclassified);
        assert_eq!(p.shown("LUA/DEP.md"), Shown::Public, "case is not a name");
    }

    #[test]
    fn without_a_key_the_default_and_without_a_default_nothing() {
        let p = policy(&[("lua", Public)], Some(Internal));
        assert_eq!(p.shown("native/REL.md"), Shown::Internal);
        assert_eq!(p.classify("native/REL.md").key, None);
        let p = policy(&[("lua", Public)], None);
        assert_eq!(p.shown("native/REL.md"), Shown::Unclassified);
        assert!(!Shown::Unclassified.released());
        assert!(!Shown::Private.released());
        assert!(Shown::Internal.released() && Shown::Public.released());
    }

    #[test]
    fn a_numbered_name_is_never_classified() {
        // `Checklists#2` follows the order of the configuration: it can be another rule set
        // tomorrow. Not with a key, not with the default.
        let p = policy(
            &[("Checklists", Public), ("wkdbook-Lua", Public)],
            Some(Public),
        );
        for source in [
            "Checklists#2/DEP.md",
            "Checklists#3",
            "wkdbook-Lua/Checklists#2/DEP.md",
            "DEP.md#2",
        ] {
            let v = p.classify(source);
            assert_eq!(v.shown, Shown::Unclassified, "{source}");
            assert_eq!(v.key, None, "{source}");
        }
        // A # further in is a file, not a name.
        assert_eq!(p.shown("Checklists/_rules/C#/X.md"), Shown::Public);
    }

    #[test]
    fn a_source_not_of_the_engine_s_shape_is_never_classified() {
        let p = policy(&[("lua", Public)], Some(Public));
        for source in [
            "",
            "/lua/DEP.md",
            "<external>/DEP.md",
            "lua/../native/DEP.md",
            "../lua/DEP.md",
        ] {
            assert_eq!(p.shown(source), Shown::Unclassified, "{source:?}");
        }
    }

    #[test]
    fn a_key_of_the_other_name_of_a_rule_set_only_makes_it_stricter() {
        // Keyed while it was alone, then a second `Checklists` is loaded: the private rule set is
        // now `wkdbook-Lua/Checklists`, and the default would release it.
        let p = policy(&[("Checklists", Private)], Some(Public));
        let v = p.classify("wkdbook-Lua/Checklists/DEP.md");
        assert_eq!(v.shown, Shown::Private);
        assert_eq!(v.key, Some("Checklists"));
        // Keyed with its folder, then the other `Checklists` is gone: it is `Checklists` again.
        let p = policy(&[("wkdbook-Lua/Checklists", Private)], Some(Public));
        assert_eq!(p.shown("Checklists/DEP.md"), Shown::Private);
        // Only stricter: a public key of the other name does not loosen a key or the default.
        let p = policy(&[("Checklists", Public)], Some(Private));
        assert_eq!(p.shown("wkdbook-Lua/Checklists/DEP.md"), Shown::Private);
        let p = policy(&[("Checklists", Public)], None);
        assert_eq!(
            p.shown("wkdbook-Lua/Checklists/DEP.md"),
            Shown::Unclassified
        );
        let p = policy(&[("wkdbook-Lua", Internal), ("Checklists", Public)], None);
        assert_eq!(p.shown("wkdbook-Lua/Checklists/DEP.md"), Shown::Internal);
        // A two-part key names one parent: under another parent its name is no alias.
        let p = policy(&[("other/Checklists", Private)], Some(Public));
        assert_eq!(p.shown("wkdbook-Lua/Checklists/DEP.md"), Shown::Public);
        // A key of a folder inside the rule set, written while it was alone, keeps that folder
        // once the rule set is renamed: the default would release it.
        let p = policy(&[("Checklists/_rules", Private)], Some(Public));
        assert_eq!(p.shown("Checklists/_rules/ERR/ERR-100.md"), Shown::Private);
        let v = p.classify("wkdbook-Lua/Checklists/_rules/ERR/ERR-100.md");
        assert_eq!(v.shown, Shown::Private);
        assert_eq!(v.key, Some("Checklists/_rules"));
        assert_eq!(p.shown("wkdbook-Lua/Checklists/DEP.md"), Shown::Public);
        assert_eq!(
            p.shown("wkdbook-Lua/Checklists/_rules2/X.md"),
            Shown::Public
        );
        // Still only stricter.
        let p = policy(&[("Checklists/_rules", Public)], Some(Internal));
        assert_eq!(
            p.shown("wkdbook-Lua/Checklists/_rules/ERR/ERR-100.md"),
            Shown::Internal
        );
    }

    #[test]
    fn without_a_default_an_explicit_private_of_the_other_name_is_kept() {
        // Nobody said anything about the new name, but the owner marked the rule set private:
        // it is shown as private, with its key, not as "nobody said".
        let p = policy(&[("Checklists", Private)], None);
        let v = p.classify("wkdbook-Lua/Checklists/DEP.md");
        assert_eq!(v.shown, Shown::Private);
        assert_eq!(v.key, Some("Checklists"));
        let p = policy(&[("wkdbook-Lua/Checklists", Private)], None);
        assert_eq!(p.shown("Checklists/DEP.md"), Shown::Private);
        let files = [SourceFile {
            path: "wkdbook-Lua/Checklists/DEP.md".to_owned(),
            rules: 1,
        }];
        let doc = policy(&[("Checklists", Private)], None).document(&files);
        assert_eq!(doc["private"], json!(["wkdbook-Lua/Checklists/DEP.md"]));
        assert_eq!(doc["unclassified"], json!([]));
        // A released class of the other name never replaces "nobody said".
        for class in [Public, Internal] {
            let p = policy(&[("Checklists", class)], None);
            let v = p.classify("wkdbook-Lua/Checklists/DEP.md");
            assert_eq!(v.shown, Shown::Unclassified, "{class:?}");
            assert_eq!(v.key, None, "{class:?}");
        }
    }

    #[test]
    fn the_document_classifies_every_file() {
        let p = policy(
            &[
                ("Checklists", Internal),
                ("Checklists/_rules", Private),
                ("lua", Public),
            ],
            None,
        );
        let files = [
            SourceFile {
                path: "Checklists/DEP.md".to_owned(),
                rules: 2,
            },
            SourceFile {
                path: "Checklists/_rules/ERR/ERR-100.md".to_owned(),
                rules: 1,
            },
            SourceFile {
                path: "native/REL.md".to_owned(),
                rules: 3,
            },
        ];
        let doc = p.document(&files);
        assert_eq!(doc["schema"], 1);
        assert_eq!(doc["kind"], "hub.sources");
        assert_eq!(
            doc["files"],
            json!([
                { "path": "Checklists/DEP.md", "data_class": "internal", "rules": 2,
                  "key": "Checklists" },
                { "path": "Checklists/_rules/ERR/ERR-100.md", "data_class": "private",
                  "rules": 1, "key": "Checklists/_rules" },
                { "path": "native/REL.md", "data_class": "unclassified", "rules": 3,
                  "key": null },
            ])
        );
        assert_eq!(
            doc["keys"],
            json!([
                { "key": "Checklists", "class": "internal", "files": 2 },
                { "key": "Checklists/_rules", "class": "private", "files": 1 },
                { "key": "lua", "class": "public", "files": 0 },
            ])
        );
        assert_eq!(doc["unclassified"], json!(["native/REL.md"]));
        assert_eq!(doc["private"], json!(["Checklists/_rules/ERR/ERR-100.md"]));
    }
}
