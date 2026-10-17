//! What the hub keeps while it runs: the status `hello` reports, the engine, the snapshot and the
//! data classes of this device.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use appkit_hub::{HubStatus, Runtime};
use axum::body::Bytes;
use serde::Deserialize;
use serde_json::Value;

use crate::engine::Engine;
use crate::sources::{SourceFile, SourcePolicy};
use crate::store::{Cached, SnapshotStore, StoreOptions, SHAPE};

/// What the hub keeps for the vault it reads.
pub struct RulesRuntime {
    pub(crate) status: HubStatus,
    pub(crate) engine: Option<Arc<dyn Engine>>,
    pub(crate) store: Arc<SnapshotStore>,
    /// The data classes of the rule sets on this device (`sources.json`).
    pub(crate) sources: Arc<SourcePolicy>,
    /// What was read of the held snapshot, with the text it was read from.
    held: Mutex<Option<(Bytes, Arc<Held>)>>,
}

/// What the routes read of a snapshot: the ids of its rules and its files.
pub(crate) struct Held {
    pub(crate) ids: HashSet<String>,
    /// `None` when the snapshot has no `sources`, or one that is not a list of files: the classes
    /// of its files are not known. The ids are read all the same.
    pub(crate) files: Option<Vec<SourceFile>>,
    /// The answer of the `sources` route for these files, computed once for the snapshot.
    pub(crate) sources_body: OnceLock<Bytes>,
}

impl Runtime for RulesRuntime {
    type Target = ();

    fn status(&self) -> &HubStatus {
        &self.status
    }

    fn stop(&self) {}
}

/// A snapshot read for the ids of its rules and its files, and nothing else.
#[derive(Deserialize)]
struct SnapshotParts {
    rules: Vec<RuleId>,
    /// Read apart from the rules: a file that is not right must not cost the snapshot its ids.
    #[serde(default)]
    sources: Option<Value>,
}

#[derive(Deserialize)]
struct RuleId {
    id: String,
}

impl RulesRuntime {
    pub(crate) fn new(
        engine: Option<Arc<dyn Engine>>,
        status: HubStatus,
        sources: Arc<SourcePolicy>,
        max_age: Option<Duration>,
    ) -> Self {
        let store = SnapshotStore::new(
            engine.clone(),
            SHAPE,
            StoreOptions {
                max_age,
                ..StoreOptions::default()
            },
        );
        Self {
            status,
            engine,
            store,
            sources,
            held: Mutex::new(None),
        }
    }

    /// What the routes read of a snapshot; `None` when its text is not a snapshot with a list of
    /// rules. Read once for each snapshot the store holds: the text is kept with it, so the same
    /// allocation means the same document.
    pub(crate) fn held(&self, cached: &Cached) -> Option<Arc<Held>> {
        let mut held = self
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((text, parts)) = held.as_ref()
            && text.as_ptr() == cached.text.as_ptr()
            && text.len() == cached.text.len()
        {
            return Some(Arc::clone(parts));
        }
        let parts = Arc::new(read_held(&cached.text)?);
        *held = Some((cached.text.clone(), Arc::clone(&parts)));
        Some(parts)
    }

    /// The answer of the `sources` route for a held snapshot; `None` when its files are not known.
    /// The policy does not change while the hub runs, so it is computed once for each snapshot.
    pub(crate) fn sources_body(&self, held: &Held) -> Option<Bytes> {
        let files = held.files.as_ref()?;
        Some(
            held.sources_body
                .get_or_init(|| Bytes::from(self.sources.document(files).to_string()))
                .clone(),
        )
    }
}

/// What the routes read of a snapshot's text; `None` without a list of rules.
fn read_held(text: &[u8]) -> Option<Held> {
    let doc: SnapshotParts = serde_json::from_slice(text).ok()?;
    Some(Held {
        ids: doc.rules.into_iter().map(|r| r.id).collect(),
        files: doc
            .sources
            .and_then(|v| serde_json::from_value::<Vec<SourceFile>>(v).ok()),
        sources_body: OnceLock::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::Class;

    fn runtime() -> RulesRuntime {
        let policy = SourcePolicy::new([("lua".to_owned(), Class::Public)]).unwrap();
        let status = HubStatus {
            engine_ready: false,
            problem: None,
            capabilities: Vec::new(),
        };
        RulesRuntime::new(None, status, Arc::new(policy), None)
    }

    #[test]
    fn a_broken_file_list_costs_the_snapshot_only_its_files() {
        let rt = runtime();
        for sources in [
            r#"[{"path":"lua/DEP.md"}]"#,
            r#"[{"path":"lua/DEP.md","rules":-1}]"#,
            r#"[{"path":"lua/DEP.md","rules":1},7]"#,
            r#""lua""#,
        ] {
            let text = format!(r#"{{"rules":[{{"id":"DEP-01"}}],"sources":{sources}}}"#);
            let held = read_held(text.as_bytes()).expect(sources);
            assert!(held.ids.contains("DEP-01"), "{sources}");
            assert!(held.files.is_none(), "{sources}");
            assert!(rt.sources_body(&held).is_none(), "{sources}");
        }
    }

    #[test]
    fn the_sources_answer_is_computed_once_for_a_snapshot() {
        let rt = runtime();
        let held =
            read_held(br#"{"rules":[],"sources":[{"path":"lua/DEP.md","rules":1}]}"#).unwrap();
        let first = rt.sources_body(&held).unwrap();
        let again = rt.sources_body(&held).unwrap();
        assert_eq!(first.as_ptr(), again.as_ptr(), "computed again");
        let doc: Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(doc["files"][0]["data_class"], "public");
    }
}
