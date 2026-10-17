//! The text of the rules, for the search. The kit's index asks its engine for `bodies` in pages
//! (`{ id, etag, body }` of the open documents). rules.nvim has no such call, so this wrapper answers
//! it from the two calls it has: the `snapshot` names the rules (and their `rev`), `rule` hands over
//! the text of one. Every other call goes to the engine unchanged. `bodies` is never sent to the
//! engine, and the page cannot ask for it: it is not a route.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::engine::{CallFuture, Calls, Engine, EngineError, Method, Reply};

/// The most rules read in one page, whatever the index asks for: one question to the engine is
/// asked for each, so a page is as slow as that many questions.
const PAGE_MAX: usize = 100;

/// An engine that also answers `bodies`.
pub struct BodiesEngine {
    inner: Arc<dyn Engine>,
}

impl BodiesEngine {
    #[must_use]
    pub fn new(inner: Arc<dyn Engine>) -> Self {
        Self { inner }
    }
}

#[derive(Deserialize)]
struct Snapshot {
    rev: Option<String>,
    rules: Vec<RuleHead>,
}

#[derive(Deserialize)]
struct RuleHead {
    id: String,
    etag: Option<String>,
    #[serde(default)]
    title: String,
}

#[derive(Deserialize)]
struct RuleDoc {
    rule: RuleText,
}

#[derive(Deserialize)]
struct RuleText {
    #[serde(default)]
    text: String,
}

fn protocol(why: impl std::fmt::Display) -> EngineError {
    EngineError::Protocol(format!("bodies: {why}"))
}

impl BodiesEngine {
    async fn bodies(&self, params: Option<&Value>) -> Result<Reply, EngineError> {
        let offset = params
            .and_then(|p| p["offset"].as_u64())
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(0);
        let limit = params
            .and_then(|p| p["limit"].as_u64())
            .and_then(|n| usize::try_from(n).ok())
            .map_or(PAGE_MAX, |n| n.clamp(1, PAGE_MAX));
        let wanted: Option<Vec<&str>> = params
            .and_then(|p| p["ids"].as_array())
            .map(|ids| ids.iter().filter_map(Value::as_str).collect());

        let reply = self.inner.call(Calls::Snapshot, None).await?;
        let snapshot: Snapshot = serde_json::from_str(&reply.text).map_err(protocol)?;
        let selected: Vec<&RuleHead> = snapshot
            .rules
            .iter()
            .filter(|r| {
                wanted
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&r.id.as_str()))
            })
            .collect();
        let page: Vec<&&RuleHead> = selected.iter().skip(offset).take(limit).collect();

        let mut items = Vec::with_capacity(page.len());
        for head in &page {
            let asked = json!({ "id": head.id });
            let text = match self.inner.call(Calls::Rule, Some(&asked)).await {
                Ok(reply) => reply.text,
                // The rule went between the snapshot and now: the next change tells the index.
                Err(EngineError::Refused(r)) if r.code == "not_found" => continue,
                Err(other) => return Err(other),
            };
            let doc: RuleDoc = serde_json::from_str(&text).map_err(protocol)?;
            items.push(json!({
                "id": head.id,
                "etag": head.etag,
                // The title is searched too, and the text under it.
                "body": format!("{}\n{}", head.title, doc.rule.text),
            }));
        }
        let end = offset.saturating_add(page.len());
        let next_offset = (end < selected.len()).then_some(end);
        Ok(Reply {
            kind: Calls::Bodies.reply_kind().to_owned(),
            text: json!({
                "schema": 1,
                "kind": "rules.bodies",
                "rev": snapshot.rev,
                "items": items,
                "next_offset": next_offset,
            })
            .to_string(),
        })
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Three rules; asks are kept.
    struct Fake {
        asked: Mutex<Vec<String>>,
    }

    impl Engine for Fake {
        fn call<'a>(&'a self, method: Method, params: Option<&'a Value>) -> CallFuture<'a> {
            Box::pin(async move {
                self.asked.lock().unwrap().push(method.as_str().to_owned());
                let text = match method.as_str() {
                    "snapshot" => json!({"rev":"rev:1","rules":[
                        {"id":"A-01","etag":"e1","title":"first title"},
                        {"id":"A-02","etag":"e1","title":"second title"},
                        {"id":"B-01","etag":"e2","title":"third title"}]})
                    .to_string(),
                    "rule" => {
                        let id = params.unwrap()["id"].as_str().unwrap();
                        json!({"rule":{"id":id,"text":format!("text of {id}")}}).to_string()
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

    fn engine() -> (Arc<Fake>, BodiesEngine) {
        let fake = Arc::new(Fake {
            asked: Mutex::new(Vec::new()),
        });
        (
            Arc::clone(&fake),
            BodiesEngine::new(fake as Arc<dyn Engine>),
        )
    }

    async fn page(e: &BodiesEngine, params: Value) -> Value {
        let reply = e.call(Calls::Bodies, Some(&params)).await.unwrap();
        assert_eq!(reply.kind, "rules.bodies");
        serde_json::from_str(&reply.text).unwrap()
    }

    #[tokio::test]
    async fn a_page_holds_the_title_and_the_text_of_each_rule() {
        let (_, e) = engine();
        let doc = page(&e, json!({ "offset": 0, "limit": 1000 })).await;
        assert_eq!(doc["rev"], "rev:1");
        assert_eq!(doc["items"].as_array().unwrap().len(), 3);
        assert_eq!(doc["items"][0]["id"], "A-01");
        assert_eq!(doc["items"][0]["etag"], "e1");
        assert_eq!(doc["items"][0]["body"], "first title\ntext of A-01");
        assert!(doc["next_offset"].is_null());
    }

    #[tokio::test]
    async fn pages_follow_each_other_by_offset() {
        let (_, e) = engine();
        let first = page(&e, json!({ "offset": 0, "limit": 2 })).await;
        assert_eq!(first["items"].as_array().unwrap().len(), 2);
        assert_eq!(first["next_offset"], 2);
        let second = page(&e, json!({ "offset": 2, "limit": 2 })).await;
        assert_eq!(second["items"][0]["id"], "B-01");
        assert!(second["next_offset"].is_null());
    }

    #[tokio::test]
    async fn named_ids_are_the_only_ones_read() {
        let (fake, e) = engine();
        let doc = page(
            &e,
            json!({ "ids": ["B-01", "nope"], "offset": 0, "limit": 10 }),
        )
        .await;
        let ids: Vec<&str> = doc["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["B-01"]);
        let rules = fake
            .asked
            .lock()
            .unwrap()
            .iter()
            .filter(|m| *m == "rule")
            .count();
        assert_eq!(rules, 1, "a rule that was not asked for was read");
    }

    #[tokio::test]
    async fn a_limit_of_zero_is_a_page_of_one() {
        let (_, e) = engine();
        let doc = page(&e, json!({ "limit": 0 })).await;
        assert_eq!(
            doc["items"].as_array().unwrap().len(),
            1,
            "a limit of 0 is 1"
        );
    }

    #[tokio::test]
    async fn every_other_call_goes_to_the_engine_unchanged() {
        let (fake, e) = engine();
        let reply = e.call(Calls::Snapshot, None).await.unwrap();
        assert!(reply.text.contains("rev:1"));
        assert_eq!(*fake.asked.lock().unwrap(), ["snapshot"]);
    }
}
