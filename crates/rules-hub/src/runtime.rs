//! What the hub keeps while it runs: the status `hello` reports, the engine and the snapshot.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use appkit_hub::{HubStatus, Runtime};
use axum::body::Bytes;
use serde::Deserialize;

use crate::engine::Engine;
use crate::store::{Cached, SnapshotStore, StoreOptions, SHAPE};

/// What the hub keeps for the vault it reads.
pub struct RulesRuntime {
    pub(crate) status: HubStatus,
    pub(crate) engine: Option<Arc<dyn Engine>>,
    pub(crate) store: Arc<SnapshotStore>,
    /// The ids of the held snapshot, with the text they were read from.
    ids: Mutex<Option<(Bytes, Arc<HashSet<String>>)>>,
}

impl Runtime for RulesRuntime {
    type Target = ();

    fn status(&self) -> &HubStatus {
        &self.status
    }

    fn stop(&self) {}
}

/// A snapshot read for the ids of its rules and nothing else.
#[derive(Deserialize)]
struct SnapshotIds {
    rules: Vec<RuleId>,
}

#[derive(Deserialize)]
struct RuleId {
    id: String,
}

impl RulesRuntime {
    pub(crate) fn new(
        engine: Option<Arc<dyn Engine>>,
        status: HubStatus,
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
            ids: Mutex::new(None),
        }
    }

    /// The ids of the rules of a snapshot; `None` when its text is not a snapshot. Read once for
    /// each snapshot the store holds: the text is kept with them, so the same allocation means the
    /// same document.
    pub(crate) fn rule_ids(&self, cached: &Cached) -> Option<Arc<HashSet<String>>> {
        let mut held = self
            .ids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((text, ids)) = held.as_ref()
            && text.as_ptr() == cached.text.as_ptr()
            && text.len() == cached.text.len()
        {
            return Some(Arc::clone(ids));
        }
        let doc: SnapshotIds = serde_json::from_slice(&cached.text).ok()?;
        let ids: Arc<HashSet<String>> = Arc::new(doc.rules.into_iter().map(|r| r.id).collect());
        *held = Some((cached.text.clone(), Arc::clone(&ids)));
        Some(ids)
    }
}
