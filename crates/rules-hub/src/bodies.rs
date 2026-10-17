//! The text of the rules for the search index, without a call of the engine for it.
//!
//! The kit's index asks the call [`Calls::Bodies`] for pages `{rev, items: [{id, etag, body}],
//! next_offset}`, for `{offset, limit}` (every document) and `{ids, offset, limit}` (the named
//! ones). `rules-export/1` has no such call, so [`BodiesEngine`] answers it inside the hub: it asks
//! `snapshot` for the ids and `rule` for each text, and passes every other call on unchanged. The
//! page it writes never leaves the hub; it only feeds the index.
//!
//! - A page is read from one state of the rule sets: every `rule` document must carry the `rev`
//!   of the snapshot the ids came from, else the page is read again (at most [`PAGE_TRIES`]
//!   times) and then refused, and the index keeps what it had. The index itself only compares the
//!   `rev` between pages.
//! - A page costs a `rule` question per rule. Without a running host each would be a Neovim
//!   process of its own, so the text is only read while the engine's host runs; otherwise the call
//!   is refused and the search says it failed (the title search of the page goes on).

use std::collections::HashSet;
use std::sync::Arc;

use appkit_hub::BoxFuture;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::engine::{CallFuture, Calls, CliEngine, Engine, EngineError, Method, Reply};

/// Reads of a page before it is refused, when the rules keep changing under it.
pub const PAGE_TRIES: usize = 3;
/// The most rules of one page (the index asks for 1000).
pub const MAX_PAGE: usize = 1000;

/// An engine whose answers may come from a persistent host: the text of every rule is asked one
/// rule at a time, which only a host answers cheaply.
pub trait HostedEngine: Engine {
    /// Whether a persistent host process answers right now.
    fn host_running(&self) -> BoxFuture<'_, bool>;
}

impl HostedEngine for CliEngine {
    fn host_running(&self) -> BoxFuture<'_, bool> {
        Box::pin(CliEngine::host_running(self))
    }
}

/// The engine with the call `bodies` answered in the hub (see the module).
pub struct BodiesEngine {
    inner: Arc<dyn HostedEngine>,
}

impl BodiesEngine {
    #[must_use]
    pub fn new(inner: Arc<dyn HostedEngine>) -> Self {
        Self { inner }
    }

    async fn bodies(&self, params: Option<&Value>) -> Result<Reply, EngineError> {
        let request = BodiesRequest::read(params)?;
        if !self.inner.host_running().await {
            return Err(EngineError::Spawn(
                "the texts of the rules are read only while the engine's host runs".to_owned(),
            ));
        }
        for _ in 0..PAGE_TRIES {
            if let Some(page) = self.read_page(&request).await? {
                return Ok(Reply {
                    kind: Calls::Bodies.reply_kind().to_owned(),
                    text: page.to_string(),
                });
            }
        }
        Err(EngineError::Protocol(
            "the rules kept changing while their texts were read".to_owned(),
        ))
    }

    /// One page from one state of the rules; `None` when the state changed while it was read.
    async fn read_page(&self, request: &BodiesRequest) -> Result<Option<Value>, EngineError> {
        let reply = self.inner.call(Calls::Snapshot, None).await?;
        let snapshot: SnapshotIds = serde_json::from_str(&reply.text)
            .map_err(|e| EngineError::Protocol(format!("a snapshot without its rules: {e}")))?;
        let all: Vec<String> = snapshot.rules.into_iter().map(|r| r.id).collect();
        // The named ids in the order they were named, each once and only those of the snapshot:
        // no id that is not one of its rules reaches the engine.
        let wanted: Vec<String> = match &request.ids {
            None => all,
            Some(ids) => {
                let known: HashSet<&str> = all.iter().map(String::as_str).collect();
                let mut seen = HashSet::new();
                ids.iter()
                    .filter(|id| known.contains(id.as_str()) && seen.insert(id.as_str()))
                    .cloned()
                    .collect()
            }
        };
        let start = usize::try_from(request.offset)
            .unwrap_or(usize::MAX)
            .min(wanted.len());
        let end = start.saturating_add(request.limit).min(wanted.len());
        let mut items = Vec::with_capacity(end - start);
        for id in &wanted[start..end] {
            let rule = match self
                .inner
                .call(Calls::Rule, Some(&json!({ "id": id })))
                .await
            {
                Ok(reply) => reply,
                Err(EngineError::Refused(r)) if r.code == "not_found" => {
                    if request.ids.is_some() {
                        // A named rule that is gone: the change that named it also removes it.
                        continue;
                    }
                    // A rule of the snapshot that is gone: the state changed under the page.
                    return Ok(None);
                }
                Err(e) => return Err(e),
            };
            let doc: RuleText = serde_json::from_str(&rule.text)
                .map_err(|e| EngineError::Protocol(format!("a rule without its text: {e}")))?;
            if doc.rev != snapshot.rev {
                return Ok(None);
            }
            items.push(json!({ "id": id, "etag": doc.rule.etag, "body": doc.rule.text }));
        }
        let next_offset = (end < wanted.len()).then_some(end);
        Ok(Some(json!({
            "schema": 1,
            "kind": Calls::Bodies.reply_kind(),
            "rev": snapshot.rev,
            "items": items,
            "next_offset": next_offset,
        })))
    }
}

impl Engine for BodiesEngine {
    fn call<'a>(&'a self, method: Method, params: Option<&'a Value>) -> CallFuture<'a> {
        if method == Calls::Bodies {
            Box::pin(self.bodies(params))
        } else {
            self.inner.call(method, params)
        }
    }
}

/// What the index asks for: a page of every rule, or of the named ones.
struct BodiesRequest {
    ids: Option<Vec<String>>,
    offset: u64,
    limit: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BodiesParams {
    #[serde(default)]
    ids: Option<Vec<String>>,
    #[serde(default)]
    offset: u64,
    limit: u64,
}

impl BodiesRequest {
    fn read(params: Option<&Value>) -> Result<Self, EngineError> {
        let params = params.ok_or_else(|| EngineError::BadRequest("bodies needs a page".into()))?;
        let p: BodiesParams = serde_json::from_value(params.clone())
            .map_err(|e| EngineError::BadRequest(format!("bodies: {e}")))?;
        let limit = usize::try_from(p.limit)
            .unwrap_or(MAX_PAGE)
            .clamp(1, MAX_PAGE);
        Ok(Self {
            ids: p.ids,
            offset: p.offset,
            limit,
        })
    }
}

#[derive(Deserialize)]
struct SnapshotIds {
    rev: Option<String>,
    rules: Vec<RuleId>,
}

#[derive(Deserialize)]
struct RuleId {
    id: String,
}

#[derive(Deserialize)]
struct RuleText {
    rev: Option<String>,
    rule: RuleBody,
}

#[derive(Deserialize)]
struct RuleBody {
    etag: Option<String>,
    #[serde(default)]
    text: String,
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    use super::*;
    use crate::engine::EngineRefusal;

    /// Rules by id with a text; `rev` of the snapshot and of every rule document, which a test may
    /// let move after a number of `rule` answers.
    struct Fake {
        rules: Mutex<Vec<(&'static str, &'static str)>>,
        rev: Mutex<String>,
        /// After this many `rule` answers the rev moves on once (a change while a page is read).
        move_after: Mutex<Option<usize>>,
        host: AtomicBool,
        asked: Mutex<Vec<String>>,
        rule_calls: AtomicUsize,
    }

    impl Fake {
        fn new(rules: &[(&'static str, &'static str)]) -> Arc<Self> {
            Arc::new(Self {
                rules: Mutex::new(rules.to_vec()),
                rev: Mutex::new("rev:1".to_owned()),
                move_after: Mutex::new(None),
                host: AtomicBool::new(true),
                asked: Mutex::new(Vec::new()),
                rule_calls: AtomicUsize::new(0),
            })
        }

        fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }
    }

    impl Engine for Fake {
        fn call<'a>(&'a self, method: Method, params: Option<&'a Value>) -> CallFuture<'a> {
            Box::pin(async move {
                let name = method.as_str();
                let shown = match params {
                    Some(p) => format!("{name} {p}"),
                    None => name.to_owned(),
                };
                self.asked.lock().unwrap().push(shown);
                let rev = self.rev.lock().unwrap().clone();
                let text = match name {
                    "snapshot" => {
                        let rules: Vec<Value> = self
                            .rules
                            .lock()
                            .unwrap()
                            .iter()
                            .map(|(id, _)| json!({ "id": id, "etag": "sha256:1" }))
                            .collect();
                        json!({ "rev": rev, "rules": rules }).to_string()
                    }
                    "rule" => {
                        let n = self.rule_calls.fetch_add(1, Ordering::SeqCst) + 1;
                        let id = params.and_then(|p| p["id"].as_str()).unwrap();
                        let found = self
                            .rules
                            .lock()
                            .unwrap()
                            .iter()
                            .find(|(i, _)| *i == id)
                            .map(|(_, text)| *text);
                        let Some(text) = found else {
                            return Err(EngineError::Refused(EngineRefusal {
                                code: "not_found".to_owned(),
                                message: format!("no rule with the id {id}"),
                                retryable: false,
                                text: String::new(),
                            }));
                        };
                        let mut moved = self.move_after.lock().unwrap();
                        if moved.is_some_and(|after| n > after) {
                            *moved = None;
                            *self.rev.lock().unwrap() = "rev:2".to_owned();
                        }
                        let rev = self.rev.lock().unwrap().clone();
                        json!({ "rev": rev, "rule": { "etag": format!("sha256:{id}"), "text": text } })
                            .to_string()
                    }
                    other => panic!("asked {other}"),
                };
                Ok(Reply {
                    kind: method.reply_kind().to_owned(),
                    text,
                })
            })
        }
    }

    impl HostedEngine for Fake {
        fn host_running(&self) -> BoxFuture<'_, bool> {
            Box::pin(async { self.host.load(Ordering::SeqCst) })
        }
    }

    async fn page(engine: &BodiesEngine, params: Value) -> Result<Value, EngineError> {
        let reply = engine.call(Calls::Bodies, Some(&params)).await?;
        assert_eq!(reply.kind, "rules.bodies");
        Ok(serde_json::from_str(&reply.text).unwrap())
    }

    #[tokio::test]
    async fn every_rule_is_read_in_pages_with_the_rev_of_its_snapshot() {
        let fake = Fake::new(&[("A-1", "one"), ("A-2", "two"), ("A-3", "three")]);
        let engine = BodiesEngine::new(fake.clone());
        let first = page(&engine, json!({ "offset": 0, "limit": 2 }))
            .await
            .unwrap();
        assert_eq!(first["rev"], "rev:1");
        assert_eq!(
            first["items"],
            json!([
                { "id": "A-1", "etag": "sha256:A-1", "body": "one" },
                { "id": "A-2", "etag": "sha256:A-2", "body": "two" },
            ])
        );
        assert_eq!(first["next_offset"], 2);
        let last = page(&engine, json!({ "offset": 2, "limit": 2 }))
            .await
            .unwrap();
        assert_eq!(last["items"][0]["body"], "three");
        assert!(last["next_offset"].is_null(), "{last}");
        // Nothing but snapshot and rule reached the engine, and `bodies` never did.
        assert!(
            fake.asked()
                .iter()
                .all(|a| a == "snapshot" || a.starts_with("rule ")),
            "{:?}",
            fake.asked()
        );
    }

    #[tokio::test]
    async fn a_page_read_across_a_change_is_read_again() {
        let fake = Fake::new(&[("A-1", "one"), ("A-2", "two")]);
        // The second rule document is of the next state.
        *fake.move_after.lock().unwrap() = Some(1);
        let engine = BodiesEngine::new(fake.clone());
        let doc = page(&engine, json!({ "offset": 0, "limit": 10 }))
            .await
            .unwrap();
        assert_eq!(doc["rev"], "rev:2");
        assert_eq!(doc["items"].as_array().unwrap().len(), 2);
        assert_eq!(
            fake.asked().iter().filter(|a| *a == "snapshot").count(),
            2,
            "read again from a new snapshot"
        );
    }

    #[tokio::test]
    async fn a_page_that_never_holds_still_is_refused() {
        struct Moving(Arc<Fake>);
        impl Engine for Moving {
            fn call<'a>(&'a self, method: Method, params: Option<&'a Value>) -> CallFuture<'a> {
                Box::pin(async move {
                    let mut reply = self.0.call(method, params).await?;
                    if method == Calls::Rule {
                        // Every rule document is of another state than its snapshot.
                        let mut doc: Value = serde_json::from_str(&reply.text).unwrap();
                        doc["rev"] = json!("rev:elsewhere");
                        reply.text = doc.to_string();
                    }
                    Ok(reply)
                })
            }
        }
        impl HostedEngine for Moving {
            fn host_running(&self) -> BoxFuture<'_, bool> {
                self.0.host_running()
            }
        }
        let fake = Fake::new(&[("A-1", "one")]);
        let engine = BodiesEngine::new(Arc::new(Moving(fake.clone())));
        let err = page(&engine, json!({ "offset": 0, "limit": 10 }))
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::Protocol(_)), "{err}");
        assert_eq!(
            fake.asked().iter().filter(|a| *a == "snapshot").count(),
            PAGE_TRIES
        );
    }

    #[tokio::test]
    async fn named_ids_are_read_and_a_gone_one_is_left_out() {
        let fake = Fake::new(&[("A-1", "one"), ("A-2", "two")]);
        let engine = BodiesEngine::new(fake.clone());
        let doc = page(
            &engine,
            json!({ "ids": ["A-2", "NOT-IN", "A-2", "A-1"], "offset": 0, "limit": 1000 }),
        )
        .await
        .unwrap();
        let ids: Vec<&str> = doc["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["A-2", "A-1"]);
        // An id the snapshot does not hold is never asked for.
        assert!(!fake.asked().iter().any(|a| a.contains("NOT-IN")));

        // A rule of the snapshot that the engine no longer has, named by a change.
        struct Gone(Arc<Fake>);
        impl Engine for Gone {
            fn call<'a>(&'a self, method: Method, params: Option<&'a Value>) -> CallFuture<'a> {
                if method == Calls::Rule && params.is_some_and(|p| p["id"] == "A-1") {
                    return Box::pin(async {
                        Err(EngineError::Refused(EngineRefusal {
                            code: "not_found".to_owned(),
                            message: String::new(),
                            retryable: false,
                            text: String::new(),
                        }))
                    });
                }
                self.0.call(method, params)
            }
        }
        impl HostedEngine for Gone {
            fn host_running(&self) -> BoxFuture<'_, bool> {
                self.0.host_running()
            }
        }
        let engine = BodiesEngine::new(Arc::new(Gone(fake.clone())));
        let doc = page(
            &engine,
            json!({ "ids": ["A-1", "A-2"], "offset": 0, "limit": 10 }),
        )
        .await
        .unwrap();
        assert_eq!(doc["items"].as_array().unwrap().len(), 1);
        assert_eq!(doc["items"][0]["id"], "A-2");
    }

    #[tokio::test]
    async fn without_a_running_host_no_text_is_asked_for() {
        let fake = Fake::new(&[("A-1", "one")]);
        fake.host.store(false, Ordering::SeqCst);
        let engine = BodiesEngine::new(fake.clone());
        let err = page(&engine, json!({ "offset": 0, "limit": 10 }))
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::Spawn(_)), "{err}");
        assert!(fake.asked().is_empty(), "{:?}", fake.asked());
    }

    #[tokio::test]
    async fn every_other_call_is_passed_on_unchanged() {
        let fake = Fake::new(&[("A-1", "one")]);
        fake.host.store(false, Ordering::SeqCst);
        let engine = BodiesEngine::new(fake.clone());
        let reply = engine
            .call(Calls::Rule, Some(&json!({ "id": "A-1" })))
            .await
            .unwrap();
        assert_eq!(reply.kind, "rules.rule");
        engine.call(Calls::Snapshot, None).await.unwrap();
        assert_eq!(fake.asked(), [r#"rule {"id":"A-1"}"#, "snapshot"]);
    }

    #[tokio::test]
    async fn a_request_that_is_no_page_is_refused_before_anything_is_asked() {
        let fake = Fake::new(&[("A-1", "one")]);
        let engine = BodiesEngine::new(fake.clone());
        for bad in [
            None,
            Some(json!({})),
            Some(json!({ "offset": 0 })),
            Some(json!({ "limit": 1, "query": "x" })),
            Some(json!({ "ids": "A-1", "limit": 1 })),
        ] {
            let err = engine.call(Calls::Bodies, bad.as_ref()).await.unwrap_err();
            assert!(matches!(err, EngineError::BadRequest(_)), "{bad:?}: {err}");
        }
        assert!(fake.asked().is_empty());
    }
}
