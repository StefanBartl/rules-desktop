//! What the hub keeps while it runs: the status `hello` reports, the engine, the snapshot and the
//! data classes of this device.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use appkit_hub::{HubStatus, Runtime};
use axum::body::Bytes;
use serde::Deserialize;

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
    /// `None` when the snapshot has no `sources`: the classes of its files are not known.
    pub(crate) files: Option<Vec<SourceFile>>,
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
    #[serde(default)]
    sources: Option<Vec<SourceFile>>,
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
        let doc: SnapshotParts = serde_json::from_slice(&cached.text).ok()?;
        let parts = Arc::new(Held {
            ids: doc.rules.into_iter().map(|r| r.id).collect(),
            files: doc.sources,
        });
        *held = Some((cached.text.clone(), Arc::clone(&parts)));
        Some(parts)
    }
}
