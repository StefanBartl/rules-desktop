//! Which files of a build of the page (`ui/dist`) are served, and as what. One file for both
//! sides: `build.rs` includes it to embed the build, and [`crate::ui`] uses it to read a build
//! folder, so the two can never take different files. It uses `std` only, as a build script must.
//!
//! A build is its `index.html` and the files directly in `assets/`: plain names, a short list of
//! types, none over 4 MiB, all together at most 32 MiB. Nothing else next to them is part of it.

use std::path::{Path, PathBuf};

/// The most one file may be.
pub const MAX_FILE: u64 = 4 * 1024 * 1024;

/// The most all files together may be.
pub const MAX_TOTAL: u64 = 32 * 1024 * 1024;

/// One file of a build: the name the page asks for it by, where it is, and its type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `index.html` or `assets/<plain name>`.
    pub key: String,
    pub path: PathBuf,
    pub content_type: &'static str,
}

/// The type a file is served with, by its extension; `None` for every other file.
#[must_use]
pub fn content_type(name: &str) -> Option<&'static str> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        _ => return None,
    })
}

/// A name a build gives a file: letters, digits, `.`, `_` and `-`, not starting with a dot, at
/// most 128 bytes. No separator, so a name can never reach out of `assets/`.
#[must_use]
pub fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The files of the build in `dist`, sorted by key. A folder without an `index.html` is no build:
/// no entries.
///
/// # Errors
///
/// A file that cannot be looked at, one over [`MAX_FILE`], or all over [`MAX_TOTAL`]: a build that
/// went wrong is not served as if it were whole.
pub fn entries(dist: &Path) -> std::io::Result<Vec<Entry>> {
    let index = dist.join("index.html");
    if !index.is_file() {
        return Ok(Vec::new());
    }
    let mut out = vec![Entry {
        key: "index.html".to_owned(),
        path: index,
        content_type: "text/html; charset=utf-8",
    }];
    if let Ok(dir) = std::fs::read_dir(dist.join("assets")) {
        for item in dir {
            let item = item?;
            let Some(name) = item.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Some(content_type) = content_type(&name) else {
                continue;
            };
            let path = item.path();
            if plain_name(&name) && path.is_file() {
                out.push(Entry {
                    key: format!("assets/{name}"),
                    path,
                    content_type,
                });
            }
        }
    }
    let mut total = 0u64;
    for entry in &out {
        let len = std::fs::metadata(&entry.path)?.len();
        if len > MAX_FILE {
            return Err(std::io::Error::other(format!(
                "{} is larger than {MAX_FILE} bytes",
                entry.path.display()
            )));
        }
        total += len;
    }
    if total > MAX_TOTAL {
        return Err(std::io::Error::other(format!(
            "the files of the page are larger than {MAX_TOTAL} bytes together"
        )));
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(out)
}
