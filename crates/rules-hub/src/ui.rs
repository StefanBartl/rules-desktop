//! The files of the page, held in memory.
//!
//! The built page (`ui/dist`) is embedded in the binary by `build.rs`. Either way the files sit in
//! a table keyed by the name the page asks for, built by [`UiFiles::from_entries`]: from the
//! embedded files, or from a build folder ([`UiFiles::load`], which the tests fill with files of
//! their own). A request is answered from the table or not at all: no path is ever joined and
//! opened per request, so there is nothing to traverse, and no folder is searched in which a
//! script could be planted. Which files a build holds is [`crate::ui_table`], shared with
//! `build.rs`.

use std::collections::HashMap;
use std::path::Path;

use appkit_hub::{Page, Pages};
use axum::body::Bytes;

use crate::ui_table;

include!(concat!(env!("OUT_DIR"), "/ui_embedded.rs"));

/// The page: its files by key (`index.html`, `assets/<name>`).
#[derive(Clone, Default)]
pub struct UiFiles {
    files: HashMap<String, Page>,
}

impl UiFiles {
    /// No page: the hub answers with the placeholder of the kit.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// The table of these files, by key. A key the page could not ask for (anything but
    /// `index.html` and `assets/<plain name>`) is left out.
    #[must_use]
    pub fn from_entries(entries: impl IntoIterator<Item = (String, Page)>) -> Self {
        let files = entries
            .into_iter()
            .filter(|(key, _)| servable_key(key))
            .collect();
        Self { files }
    }

    /// The page that is part of this binary (none when it was built without `ui/dist`).
    #[must_use]
    pub fn embedded() -> Self {
        Self::from_entries(EMBEDDED.iter().map(|(key, content_type, bytes)| {
            (
                (*key).to_owned(),
                Page {
                    bytes: Bytes::from_static(bytes),
                    content_type,
                },
            )
        }))
    }

    /// Read a build folder: the files [`ui_table::entries`] names.
    ///
    /// # Errors
    ///
    /// The folder has no `index.html`, a file is over the limit, or one cannot be read.
    pub fn load(dir: &Path) -> std::io::Result<Self> {
        let entries = ui_table::entries(dir)?;
        if entries.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no index.html in the folder of the page",
            ));
        }
        let mut files = Vec::with_capacity(entries.len());
        for entry in entries {
            let bytes = Bytes::from(std::fs::read(&entry.path)?);
            files.push((
                entry.key,
                Page {
                    bytes,
                    content_type: entry.content_type,
                },
            ));
        }
        Ok(Self::from_entries(files))
    }

    /// The keys of the table, sorted.
    #[must_use]
    pub fn keys(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.files.keys().map(String::as_str).collect();
        keys.sort_unstable();
        keys
    }

    /// The file for a request path (`/`, `/index.html`, `/assets/x.js`).
    #[must_use]
    pub fn get(&self, request_path: &str) -> Option<&Page> {
        let key = match request_path {
            "/" => "index.html",
            other => other.strip_prefix('/')?,
        };
        self.files.get(key)
    }
}

impl Pages for UiFiles {
    fn page(&self, request_path: &str) -> Option<Page> {
        self.get(request_path).cloned()
    }

    fn has_index(&self) -> bool {
        self.files.contains_key("index.html")
    }
}

/// `index.html` or `assets/<plain name>`.
fn servable_key(key: &str) -> bool {
    key == "index.html"
        || key
            .strip_prefix("assets/")
            .is_some_and(|name| ui_table::plain_name(name) && ui_table::content_type(name).is_some())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A fresh folder with an `assets/` in it, removed when dropped.
    struct Temp(PathBuf);

    impl Temp {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "rules-desktop-ui-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("assets")).unwrap();
            Self(dir)
        }

        fn write(&self, rel: &str, text: &[u8]) {
            let path = self.0.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn page(text: &'static str) -> Page {
        Page {
            bytes: Bytes::from_static(text.as_bytes()),
            content_type: "text/plain",
        }
    }

    #[test]
    fn a_build_is_its_index_and_the_plain_files_of_a_known_type_in_assets() {
        let dir = Temp::new("entries");
        dir.write("index.html", b"<html></html>");
        dir.write("assets/index-C2Rauptk.js", b"x");
        dir.write("assets/index-C2Rauptk.css", b"x");
        dir.write("assets/Logo.SVG", b"x");
        // Next to the build or not of it: none of these is served.
        dir.write("assets/notes.txt", b"no");
        dir.write("assets/.hidden.js", b"no");
        dir.write("assets/a b.js", b"no");
        dir.write("assets/nested/deep.js", b"no");
        dir.write("secret.html", b"no");
        dir.write("other/x.js", b"no");
        let entries = ui_table::entries(&dir.0).unwrap();
        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "assets/Logo.SVG",
                "assets/index-C2Rauptk.css",
                "assets/index-C2Rauptk.js",
                "index.html"
            ]
        );
        let types: Vec<&str> = entries.iter().map(|e| e.content_type).collect();
        assert_eq!(
            types,
            [
                "image/svg+xml",
                "text/css; charset=utf-8",
                "text/javascript; charset=utf-8",
                "text/html; charset=utf-8"
            ]
        );
        let ui = UiFiles::load(&dir.0).unwrap();
        assert_eq!(ui.keys(), keys);
        assert!(ui.has_index());
    }

    #[test]
    fn a_request_is_answered_by_its_key_and_only_by_it() {
        let dir = Temp::new("serve");
        dir.write("index.html", b"<html>page</html>");
        dir.write("assets/app-1.js", b"js");
        let ui = UiFiles::load(&dir.0).unwrap();
        assert_eq!(&ui.page("/").unwrap().bytes[..], b"<html>page</html>");
        assert_eq!(
            ui.page("/index.html").unwrap().content_type,
            "text/html; charset=utf-8"
        );
        assert_eq!(&ui.page("/assets/app-1.js").unwrap().bytes[..], b"js");
        for no in [
            "",
            "index.html",
            "assets/app-1.js",
            "/assets/../index.html",
            "/assets/./app-1.js",
            "/assets/",
            "/assets",
            "/..",
            "//index.html",
            "/assets/APP-1.JS",
        ] {
            assert!(ui.page(no).is_none(), "{no}");
        }
    }

    #[test]
    fn the_table_keeps_only_keys_the_page_can_ask_for() {
        let ui = UiFiles::from_entries([
            ("index.html".to_owned(), page("i")),
            ("assets/a.js".to_owned(), page("a")),
            ("assets/../index.html".to_owned(), page("no")),
            ("assets/sub/b.js".to_owned(), page("no")),
            ("assets/notes.txt".to_owned(), page("no")),
            ("secret.html".to_owned(), page("no")),
            ("assets/.env.js".to_owned(), page("no")),
        ]);
        assert_eq!(ui.keys(), ["assets/a.js", "index.html"]);
    }

    #[test]
    fn a_folder_without_an_index_is_no_page() {
        let dir = Temp::new("empty");
        dir.write("assets/app-1.js", b"js");
        assert!(ui_table::entries(&dir.0).unwrap().is_empty());
        assert!(UiFiles::load(&dir.0).is_err());
        assert!(!UiFiles::none().has_index());
        assert!(UiFiles::none().page("/").is_none());
        // Assets without an index are not served either: the kit answers 404 under /assets/.
        let only_assets = UiFiles::from_entries([("assets/a.js".to_owned(), page("a"))]);
        assert!(!only_assets.has_index());
    }

    #[test]
    fn a_file_over_the_limit_is_an_error_not_a_hole() {
        let dir = Temp::new("big");
        dir.write("index.html", b"<html></html>");
        let big = vec![b'x'; usize::try_from(ui_table::MAX_FILE).unwrap() + 1];
        dir.write("assets/big.js", &big);
        assert!(ui_table::entries(&dir.0).is_err());
        assert!(UiFiles::load(&dir.0).is_err());
    }

    #[test]
    fn names_are_plain_and_types_are_few() {
        for ok in ["a.js", "index-C2Rauptk.js", "x_y-1.css", "a.b.c.woff2"] {
            assert!(ui_table::plain_name(ok), "{ok}");
        }
        let long = "a".repeat(129);
        for bad in [
            "",
            ".hidden",
            "a b.js",
            "a/b.js",
            "a\\b.js",
            "a%2e.js",
            "\u{e9}.js",
            long.as_str(),
        ] {
            assert!(!ui_table::plain_name(bad), "{bad}");
        }
        for (name, kind) in [
            ("a.html", Some("text/html; charset=utf-8")),
            ("a.MJS", Some("text/javascript; charset=utf-8")),
            ("a.json", Some("application/json")),
            ("a.png", Some("image/png")),
            ("a.ico", Some("image/x-icon")),
            ("a.woff2", Some("font/woff2")),
            ("a.map", None),
            ("a.txt", None),
            ("a.wasm", None),
            ("js", None),
        ] {
            assert_eq!(ui_table::content_type(name), kind, "{name}");
        }
    }

    /// `build.rs` embeds exactly what this crate would read of `ui/dist`, byte for byte: the
    /// build ran with this test's compile, so a page built before cargo is the one in the binary.
    #[test]
    fn the_embedded_page_is_the_build_in_ui_dist() {
        let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/dist");
        let embedded = UiFiles::embedded();
        let entries = ui_table::entries(&dist).unwrap();
        let want: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(embedded.keys(), want);
        assert_eq!(EMBEDDED.len(), entries.len());
        for entry in &entries {
            let file = embedded.get(&format!("/{}", entry.key)).unwrap();
            assert_eq!(file.content_type, entry.content_type, "{}", entry.key);
            assert_eq!(
                &file.bytes[..],
                std::fs::read(&entry.path).unwrap().as_slice(),
                "{}",
                entry.key
            );
        }
        if want.is_empty() {
            eprintln!("note: ui/dist is not built; the binary carries no page");
        }
    }
}
