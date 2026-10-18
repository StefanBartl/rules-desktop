//! `POST /api/rules/sources/class`: set the data class of a rule set (a key of `sources.json`) or the class of a
//! file no key names, or take a key away. The file lives in the config folder of the hub, outside the vault, and says
//! what may leave the hub: so the hub writes it itself (it is the hub's own file, not a rule set), as a whole, after
//! it has read the new text back with the rules of the file.
//!
//! A change that makes the hub **release more** than it did (a class that is more open than the one in force, or a
//! key taken away that held something back) is a loosening. It is refused with a 409 until the request says
//! `confirm_loosen: true`: the page asks the person first and says what is released; a request that does not say it
//! cannot loosen by accident.

use std::sync::Arc;

use appkit_hub::{error_body, too_many};
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use crate::routes::Shared;
use crate::sources::{loosens, loosens_default, Class, SourcePolicy};

/// What the page asks for. Exactly one of `key` and `default`; `class` null takes the key (or the default) away.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub class: Option<Class>,
    #[serde(default)]
    pub confirm_loosen: bool,
}

/// The policy after the change, or why it is not one.
///
/// # Errors
///
/// The reason, without a path.
pub fn apply(old: &SourcePolicy, change: &Change) -> Result<SourcePolicy, String> {
    match (&change.key, change.default) {
        (Some(key), false) => old.with_key(key, change.class),
        (None, true) => Ok(old.clone().with_default(change.class)),
        _ => Err("name exactly one of key and default".to_owned()),
    }
}

/// Does the change release more than the policy in force did? The strictest key wins, so a change of one key can
/// decide the class of files that a deeper key or another name of the same rule set holds back: every key of both
/// policies and every file of the held snapshot (`files`, when the hub holds one) is asked, not only the key named.
#[must_use]
pub fn is_loosening(old: &SourcePolicy, new: &SourcePolicy, files: &[String]) -> bool {
    loosens_default(old, new)
        || old
            .keys()
            .chain(new.keys())
            .map(|(key, _)| key)
            .chain(files.iter().map(String::as_str))
            .any(|source| loosens(old, new, source))
}

pub(crate) async fn set_class(State(st): State<Arc<Shared>>, body: Bytes) -> Response {
    if !st.post_limit.allow() {
        return too_many();
    }
    let change: Change = match serde_json::from_slice(&body) {
        Ok(c) => c,
        Err(_) => {
            return error_body(
                StatusCode::BAD_REQUEST,
                "bad_request",
                "the body is not a change of a class",
            )
        }
    };
    let rt = st.rt();
    let Some(dir) = &rt.config_dir else {
        return error_body(
            StatusCode::CONFLICT,
            "not_configurable",
            "this hub keeps no sources.json",
        );
    };
    // The files of the held snapshot, for the question whether a change releases more (best effort: without an engine
    // the keys alone are asked). Read before the lock: it is not held across an await.
    let files: Vec<String> = match rt.store.current().await {
        Ok(cached) if rt.store.has_engine() => rt
            .held(&cached)
            .and_then(|held| {
                held.files
                    .as_ref()
                    .map(|files| files.iter().map(|f| f.path.clone()).collect())
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    // One change at a time: read the policy in force, change it, write it, put it in force.
    let _one = rt
        .policy_edit
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let old = rt.policy();
    let new = match apply(&old, &change) {
        Ok(p) => p,
        Err(why) => return error_body(StatusCode::BAD_REQUEST, "bad_request", &why),
    };
    if is_loosening(&old, &new, &files) && !change.confirm_loosen {
        return error_body(
            StatusCode::CONFLICT,
            "confirm_needed",
            "this releases more than before: ask again with confirm_loosen",
        );
    }
    if let Err(why) = new.save(dir) {
        return error_body(StatusCode::INTERNAL_SERVER_ERROR, "write_failed", &why);
    }
    rt.set_policy(Arc::new(new));
    axum::Json(json!({ "ok": true })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(keys: &[(&str, Class)], default: Option<Class>) -> SourcePolicy {
        SourcePolicy::new(keys.iter().map(|(k, c)| ((*k).to_owned(), *c)))
            .unwrap()
            .with_default(default)
    }

    fn key(name: &str, class: Option<Class>) -> Change {
        Change {
            key: Some(name.to_owned()),
            default: false,
            class,
            confirm_loosen: false,
        }
    }

    #[test]
    fn a_key_is_set_changed_and_taken_away() {
        let old = policy(&[("a", Class::Internal)], None);
        let set = apply(&old, &key("b", Some(Class::Private))).unwrap();
        assert_eq!(
            set.keys().collect::<Vec<_>>(),
            [("a", Class::Internal), ("b", Class::Private)]
        );
        let changed = apply(&set, &key("A", Some(Class::Public))).unwrap();
        assert_eq!(
            changed.keys().collect::<Vec<_>>(),
            [("A", Class::Public), ("b", Class::Private)]
        );
        let gone = apply(&changed, &key("b", None)).unwrap();
        assert_eq!(gone.keys().collect::<Vec<_>>(), [("A", Class::Public)]);
        assert!(apply(&old, &key("bad key", Some(Class::Public))).is_err());
        assert!(apply(&old, &key("a#2", Some(Class::Public))).is_err());
    }

    #[test]
    fn the_default_is_set_or_cleared_and_exactly_one_target_is_named() {
        let old = policy(&[], None);
        let both = Change {
            key: Some("a".into()),
            default: true,
            class: Some(Class::Public),
            confirm_loosen: false,
        };
        let neither = Change {
            key: None,
            default: false,
            class: Some(Class::Public),
            confirm_loosen: false,
        };
        assert!(apply(&old, &both).is_err() && apply(&old, &neither).is_err());
        let set = apply(
            &old,
            &Change {
                key: None,
                default: true,
                class: Some(Class::Internal),
                confirm_loosen: false,
            },
        )
        .unwrap();
        assert_eq!(set.default_class(), Some(Class::Internal));
        let cleared = apply(
            &set,
            &Change {
                key: None,
                default: true,
                class: None,
                confirm_loosen: false,
            },
        )
        .unwrap();
        assert_eq!(cleared.default_class(), None);
    }

    #[test]
    fn what_releases_more_is_a_loosening_and_what_holds_back_more_is_not() {
        let old = policy(
            &[("a", Class::Internal), ("p", Class::Private)],
            Some(Class::Internal),
        );
        let loosening = |change: Change| is_loosening(&old, &apply(&old, &change).unwrap(), &[]);
        // More open than before.
        assert!(loosening(key("a", Some(Class::Public))));
        assert!(loosening(key("p", Some(Class::Internal))));
        // A key taken away falls back to the default: private to internal releases more.
        assert!(loosening(key("p", None)));
        // Stricter, or the same, is not.
        assert!(!loosening(key("a", Some(Class::Private))));
        assert!(!loosening(key("a", Some(Class::Internal))));
        assert!(
            !loosening(key("a", None)),
            "internal falls back to the internal default"
        );
        // A new key under a default that releases nothing is a loosening of that file.
        let closed = policy(&[], None);
        let open = key("n", Some(Class::Public));
        assert!(is_loosening(&closed, &apply(&closed, &open).unwrap(), &[]));
        // The default: from none to a class releases what no key named.
        let to_internal = Change {
            key: None,
            default: true,
            class: Some(Class::Internal),
            confirm_loosen: false,
        };
        assert!(is_loosening(
            &closed,
            &apply(&closed, &to_internal).unwrap(),
            &[]
        ));
        let to_none = Change {
            key: None,
            default: true,
            class: None,
            confirm_loosen: false,
        };
        assert!(!is_loosening(&old, &apply(&old, &to_none).unwrap(), &[]));
    }

    #[test]
    fn a_change_of_one_key_that_frees_files_a_deeper_key_held_back_is_a_loosening() {
        // lua/x is public but lua is private: the strictest wins, so lua/x/** is private. Without lua it is public.
        let old = policy(&[("lua", Class::Private), ("lua/x", Class::Public)], None);
        for change in [key("lua", None), key("lua", Some(Class::Public))] {
            let new = apply(&old, &change).unwrap();
            assert!(is_loosening(&old, &new, &[]), "{change:?}");
        }
        // The other name of the same rule set: Checklists holds back wkdbook-Lua/Checklists/...
        let aliased = policy(&[("Checklists", Class::Private)], Some(Class::Public));
        let gone = apply(&aliased, &key("Checklists", None)).unwrap();
        let files = ["wkdbook-Lua/Checklists/a.md".to_owned()];
        assert!(is_loosening(&aliased, &gone, &files));
        // A change that holds back more is still none.
        let tighter = apply(&old, &key("lua/x", Some(Class::Private))).unwrap();
        assert!(!is_loosening(&old, &tighter, &files));
    }

    #[test]
    fn a_body_with_a_field_it_does_not_know_is_not_a_change() {
        assert!(
            serde_json::from_str::<Change>(r#"{"key":"a","class":"private","path":"C:/x"}"#)
                .is_err()
        );
        assert!(serde_json::from_str::<Change>(r#"{"key":"a","class":"secret"}"#).is_err());
        assert!(serde_json::from_str::<Change>(r#"{"key":"a","class":null}"#).is_ok());
    }
}
