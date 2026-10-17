//! What the hub keeps while it runs: the status `hello` reports, the engine, the snapshot, the
//! search index, the data classes of this device, and the watcher and the jobs that keep the
//! snapshot and the index fresh.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use appkit_hub::search::{spawn_indexer, SearchIndex};
use appkit_hub::{HubStatus, Runtime};
use axum::body::Bytes;
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::bodies::{BodiesEngine, HostedEngine};
use crate::engine::{Calls, Engine};
use crate::events::StreamCounter;
use crate::sources::{SourceFile, SourcePolicy};
use crate::store::{spawn_refresher, Cached, RefreshOptions, SnapshotStore, StoreOptions, SHAPE};
use crate::watch::{self, Hint};

/// What the hub keeps for the vault it reads. Dropping it stops the watching and the indexing.
pub struct RulesRuntime {
    pub(crate) status: HubStatus,
    pub(crate) engine: Option<Arc<dyn Engine>>,
    pub(crate) store: Arc<SnapshotStore>,
    /// The text of the rules, kept by the hub and read through [`BodiesEngine`]; none without an
    /// engine.
    pub(crate) search: Option<Arc<SearchIndex>>,
    /// The data classes of the rule sets on this device (`sources.json`).
    pub(crate) sources: Arc<SourcePolicy>,
    /// The event streams that are open, by session token.
    pub(crate) streams: Arc<StreamCounter>,
    /// What was read of the held snapshot, with the text it was read from.
    held: Mutex<Option<(Bytes, Arc<Held>)>>,
    /// The refresher and the indexer.
    jobs: Mutex<Vec<JoinHandle<()>>>,
    // Kept alive: the refresher ends when every sender of hints is gone, and its timer and the
    // rescan must keep working even when the file watcher could not start.
    _hints: Option<mpsc::UnboundedSender<Hint>>,
    _watcher: Option<watch::Watcher>,
}

/// What a runtime is made from.
pub(crate) struct Parts {
    pub(crate) engine: Option<Arc<dyn HostedEngine>>,
    pub(crate) status: HubStatus,
    pub(crate) sources: Arc<SourcePolicy>,
    pub(crate) max_age: Option<Duration>,
    /// The folder the engine reads (`RULES_VAULT`); watched when it is given.
    pub(crate) vault: Option<PathBuf>,
    pub(crate) refresh: RefreshOptions,
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

    fn stop(&self) {
        RulesRuntime::stop(self);
    }
}

impl Drop for RulesRuntime {
    fn drop(&mut self) {
        self.stop();
    }
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
    /// The state of the hub, with its jobs started: with an engine, the refresher (woken by the
    /// watcher of the vault, its timer and the rescan the page asks for) and the search index;
    /// without one, nothing runs.
    pub(crate) fn new(parts: Parts) -> Self {
        let Parts {
            engine: hosted,
            status,
            sources,
            max_age,
            vault,
            refresh,
        } = parts;
        let engine = hosted.clone().map(|e| e as Arc<dyn Engine>);
        let store = SnapshotStore::new(
            engine.clone(),
            SHAPE,
            StoreOptions {
                max_age,
                ..StoreOptions::default()
            },
        );
        let search = hosted.map(|hosted| {
            let bodies: Arc<dyn Engine> = Arc::new(BodiesEngine::new(hosted));
            SearchIndex::new(bodies, Calls::Bodies)
        });
        let mut jobs = Vec::new();
        let (mut hints, mut watcher) = (None, None);
        if engine.is_some() {
            if let Some(index) = &search {
                jobs.push(spawn_indexer(Arc::clone(index), &store));
            }
            // The watcher only says "look again"; the refresher does the looking.
            let (tx, rx) = mpsc::unbounded_channel();
            if let Some(vault) = &vault {
                match watch::start(vault, tx.clone()) {
                    Ok(started) => watcher = Some(started),
                    // Not fatal: the timer and the rescan still work.
                    Err(e) => {
                        eprintln!(
                            "note: the vault cannot be watched ({e}); scanning on a timer only"
                        );
                    }
                }
            }
            jobs.push(spawn_refresher(Arc::clone(&store), rx, refresh));
            hints = Some(tx);
        }
        Self {
            status,
            engine,
            store,
            search,
            sources,
            streams: Arc::new(StreamCounter::default()),
            held: Mutex::new(None),
            jobs: Mutex::new(jobs),
            _hints: hints,
            _watcher: watcher,
        }
    }

    /// Stop the refresher and the indexer.
    fn stop(&self) {
        let mut jobs = self
            .jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for job in jobs.drain(..) {
            job.abort();
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
        RulesRuntime::new(Parts {
            engine: None,
            status,
            sources: Arc::new(policy),
            max_age: None,
            vault: None,
            refresh: RefreshOptions::default(),
        })
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
