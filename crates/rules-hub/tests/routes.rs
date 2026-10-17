//! The routes of the hub against an engine in the process, without Neovim: a fake that answers
//! with the golden documents of rules.nvim (`tests/fixtures/golden`, a copy of
//! `schemas/rules-export/1/golden` at the pinned commit; CI compares the two) and counts what it
//! was asked.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use appkit_core::authn::{ManualClock, Secret, CODE_BYTES, TOKEN_BYTES};
use appkit_hub::engine::{CallFuture, Engine, EngineError, EngineRefusal, Method, Reply};
use appkit_hub::{BoxFuture, HubStatus, Limits, RunningServer};
use rules_hub::bodies::HostedEngine;
use rules_hub::sources::{Class, SourcePolicy};
use rules_hub::store::RefreshOptions;
use serde_json::{json, Value};

mod common;

use common::{get, get_with, post, session, Sse};

const HELLO: &str = include_str!("fixtures/golden/hello.json");
const SNAPSHOT: &str = include_str!("fixtures/golden/snapshot.json");
const RULE: &str = include_str!("fixtures/golden/rule.json");
const ERROR: &str = include_str!("fixtures/golden/error.json");
/// What the hub answers `sources` with for the golden snapshot; the page's tests answer with it.
const HUB_SOURCES: &str = include_str!("fixtures/hub/sources.json");

/// The digest of the golden snapshot: its ETag.
const SNAPSHOT_DIGEST: &str = "sha256:9609c01610189534";

/// How long a test waits for what the hub does by itself (the first build of the search index
/// starts two seconds after the hub).
const WAIT: Duration = Duration::from_secs(15);

/// An engine in the process. It answers `hello` and `snapshot` with the golden documents, `rule`
/// with the golden rule for its id (`DEP-01`), with a rule document made from it for an id that
/// has a text in `texts`, and with a `not_found` refusal for any other, and keeps every question.
/// Its host is not running unless a test says so: then the search reads no text.
struct FakeEngine {
    snapshot: Mutex<String>,
    hello: Mutex<Result<String, ()>>,
    texts: Mutex<HashMap<String, String>>,
    host: AtomicBool,
    /// While set, `snapshot` waits for a permit of it.
    snapshot_gate: Mutex<Option<Arc<tokio::sync::Semaphore>>>,
    asked: Mutex<Vec<(&'static str, Option<Value>)>>,
}

impl FakeEngine {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            snapshot: Mutex::new(SNAPSHOT.trim_end().to_owned()),
            hello: Mutex::new(Ok(HELLO.trim_end().to_owned())),
            texts: Mutex::new(HashMap::new()),
            host: AtomicBool::new(false),
            snapshot_gate: Mutex::new(None),
            asked: Mutex::new(Vec::new()),
        })
    }

    /// An engine whose host runs and that has a text for every rule of the golden snapshot.
    fn hosted() -> Arc<Self> {
        let engine = Self::new();
        engine.host.store(true, Ordering::SeqCst);
        let mut texts = engine.texts.lock().unwrap();
        for (id, text) in [
            ("DEP-09", "Text of nine: pin every dependency."),
            ("ERR-100", "Prose of the rule."),
            ("LUA-01", "Text of the function check."),
        ] {
            texts.insert(id.to_owned(), text.to_owned());
        }
        drop(texts);
        engine
    }

    fn count(&self, method: &str) -> usize {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .filter(|(m, _)| *m == method)
            .count()
    }

    fn params_of(&self, method: &str) -> Vec<Option<Value>> {
        self.asked
            .lock()
            .unwrap()
            .iter()
            .filter(|(m, _)| *m == method)
            .map(|(_, p)| p.clone())
            .collect()
    }

    fn not_found(id: &str) -> EngineError {
        let doc: Value = serde_json::from_str(ERROR).unwrap();
        EngineError::Refused(EngineRefusal {
            code: doc["code"].as_str().unwrap().to_owned(),
            message: format!("no rule with the id {id}"),
            retryable: false,
            text: ERROR.trim_end().to_owned(),
        })
    }

    /// The `rule` document of an id: the golden one, or one made from it with another text.
    fn rule(&self, id: &str) -> Result<String, EngineError> {
        if id == "DEP-01" {
            return Ok(RULE.trim_end().to_owned());
        }
        let Some(text) = self.texts.lock().unwrap().get(id).cloned() else {
            return Err(Self::not_found(id));
        };
        let mut doc: Value = serde_json::from_str(RULE).unwrap();
        doc["rule"]["id"] = json!(id);
        doc["rule"]["text"] = json!(text);
        // Of the state the snapshot is.
        let snapshot: Value = serde_json::from_str(&self.snapshot.lock().unwrap()).unwrap();
        doc["rev"] = snapshot["rev"].clone();
        Ok(doc.to_string())
    }
}

impl Engine for FakeEngine {
    fn call<'a>(&'a self, method: Method, params: Option<&'a Value>) -> CallFuture<'a> {
        Box::pin(async move {
            self.asked
                .lock()
                .unwrap()
                .push((method.as_str(), params.cloned()));
            let text = match method.as_str() {
                "hello" => self
                    .hello
                    .lock()
                    .unwrap()
                    .clone()
                    .map_err(|()| EngineError::Timeout)?,
                "snapshot" => {
                    let gate = self.snapshot_gate.lock().unwrap().clone();
                    if let Some(gate) = gate {
                        drop(gate.acquire().await.unwrap());
                    }
                    self.snapshot.lock().unwrap().clone()
                }
                "rule" => {
                    let id = params.and_then(|p| p["id"].as_str()).unwrap_or_default();
                    self.rule(id)?
                }
                other => panic!("the fake engine was asked {other}"),
            };
            Ok(Reply {
                kind: method.reply_kind().to_owned(),
                text,
            })
        })
    }
}

impl HostedEngine for FakeEngine {
    fn host_running(&self) -> BoxFuture<'_, bool> {
        Box::pin(async { self.host.load(Ordering::SeqCst) })
    }
}

struct Hub {
    server: RunningServer,
    control: Secret,
    token: String,
}

fn ready() -> HubStatus {
    HubStatus {
        engine_ready: true,
        problem: None,
        capabilities: vec!["hello".into(), "snapshot".into(), "rule".into()],
    }
}

/// How a test starts the hub.
struct Rig {
    engine: Option<Arc<FakeEngine>>,
    max_age: Option<Duration>,
    sources: Arc<SourcePolicy>,
    vault: Option<PathBuf>,
    heartbeat: Duration,
    max_sessions: usize,
}

impl Default for Rig {
    fn default() -> Self {
        Self {
            engine: None,
            // The snapshot is held until something says it changed: each fetch is one the test
            // caused.
            max_age: None,
            sources: Arc::default(),
            vault: None,
            heartbeat: Duration::from_secs(60),
            max_sessions: Limits::default().max_sessions,
        }
    }
}

impl Rig {
    async fn start(self) -> Hub {
        let control = Secret::generate(TOKEN_BYTES).unwrap();
        let server = rules_hub::start(rules_hub::Start {
            addr: "127.0.0.1:0".parse().unwrap(),
            limits: Limits {
                snapshot_max_age: self.max_age,
                heartbeat: self.heartbeat,
                max_sessions: self.max_sessions,
                ..Limits::default()
            },
            clock: Arc::new(ManualClock::new()),
            pairing_ttl: Duration::from_secs(60),
            control_secret: control.clone(),
            instance: Secret::generate(CODE_BYTES).unwrap(),
            status: if self.engine.is_some() {
                ready()
            } else {
                HubStatus {
                    engine_ready: false,
                    problem: Some("engine_not_found"),
                    capabilities: Vec::new(),
                }
            },
            engine: self.engine.map(|e| e as Arc<dyn HostedEngine>),
            sources: self.sources,
            vault: self.vault,
            refresh: RefreshOptions {
                debounce: Duration::from_millis(100),
                max_wait: Duration::from_secs(1),
                interval: Duration::from_secs(300),
            },
        })
        .await
        .expect("the hub starts");
        let token = session(&server, &control).await;
        Hub {
            server,
            control,
            token,
        }
    }
}

async fn start_with(engine: Option<Arc<FakeEngine>>, max_age: Option<Duration>) -> Hub {
    Rig {
        engine,
        max_age,
        ..Rig::default()
    }
    .start()
    .await
}

async fn start_classified(
    engine: Option<Arc<FakeEngine>>,
    max_age: Option<Duration>,
    sources: Arc<SourcePolicy>,
) -> Hub {
    Rig {
        engine,
        max_age,
        sources,
        ..Rig::default()
    }
    .start()
    .await
}

async fn start(engine: &Arc<FakeEngine>) -> Hub {
    start_with(Some(Arc::clone(engine)), None).await
}

fn route(name: &str) -> String {
    rules_hub::IDENTITY.route(name)
}

impl Hub {
    async fn get(&self, path: &str) -> appkit_testkit::wire::Reply {
        get(&self.server, path, Some(&self.token)).await
    }

    async fn post(&self, path: &str) -> appkit_testkit::wire::Reply {
        post(&self.server, path, Some(&self.token), "{}").await
    }

    async fn stream(&self) -> Sse {
        let mut sse = Sse::open(&self.server, Some(&self.token))
            .await
            .expect("the stream opens");
        sse.next_event("hello", WAIT).await.expect("its hello");
        sse
    }
}

#[tokio::test]
async fn every_route_of_the_table_is_served_to_a_session_and_to_nobody_else() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    let mut seen = HashMap::new();
    for name in rules_hub::DATA_ROUTES.iter() {
        let without = get(&hub.server, &route(name), None).await;
        assert_eq!(without.status, 401, "{name}: {without:?}");
        let status = match *name {
            // A stream does not end: its first message says it is served.
            "events" => {
                assert_eq!(Sse::open(&hub.server, None).await.err(), Some(401));
                let mut sse = Sse::open(&hub.server, Some(&hub.token)).await.unwrap();
                assert!(sse.next_event("hello", WAIT).await.is_some());
                200
            }
            "rescan" => {
                let no_session = post(&hub.server, &route(name), None, "{}").await;
                assert_eq!(no_session.status, 401, "{no_session:?}");
                hub.post(&route(name)).await.status
            }
            _ => hub.get(&route(name)).await.status,
        };
        // Registered: a route that is not would be a 404 of the router with an empty body.
        assert!(
            status == 200 || status == 202 || status == 400,
            "{name}: {status}"
        );
        seen.insert(*name, status);
    }
    assert_eq!(seen.get("snapshot"), Some(&200));
    // `rule` and `search` without their query are a bad query, told by the route.
    assert_eq!(seen.get("rule"), Some(&400));
    assert_eq!(seen.get("search"), Some(&400));
    assert_eq!(seen.get("sources"), Some(&200));
    assert_eq!(seen.get("events"), Some(&200));
    assert_eq!(seen.get("rescan"), Some(&202));
    assert_eq!(seen.get("about"), Some(&200));
}

#[tokio::test]
async fn the_snapshot_is_the_engine_s_document_with_its_digest_as_etag() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    let r = hub.get(&route("snapshot")).await;
    assert_eq!(r.status, 200, "{r:?}");
    assert_eq!(r.body, SNAPSHOT.trim_end(), "passed on byte for byte");
    let quoted = format!("\"{SNAPSHOT_DIGEST}\"");
    assert_eq!(r.header("etag"), Some(quoted.as_str()));
    assert_eq!(r.header("content-type"), Some("application/json"));

    // The tag the client holds: nothing new, no body.
    for held in [
        quoted.clone(),
        format!("W/{quoted}"),
        "\"x\", ".to_owned() + &quoted,
    ] {
        let again = get_with(
            &hub.server,
            &route("snapshot"),
            Some(&hub.token),
            &[("If-None-Match", held.as_str())],
        )
        .await;
        assert_eq!(again.status, 304, "{held}: {again:?}");
        assert!(again.body.is_empty(), "{again:?}");
        assert_eq!(again.header("etag"), Some(quoted.as_str()));
    }
    let other = get_with(
        &hub.server,
        &route("snapshot"),
        Some(&hub.token),
        &[("If-None-Match", "\"sha256:0000000000000000\"")],
    )
    .await;
    assert_eq!(other.status, 200);
    assert_eq!(other.body, SNAPSHOT.trim_end());
    // The snapshot was read once and held for every answer.
    assert_eq!(engine.count("snapshot"), 1);

    let with_query = hub.get(&format!("{}?x=1", route("snapshot"))).await;
    assert_eq!(with_query.status, 400, "{with_query:?}");
}

#[tokio::test]
async fn a_rule_of_the_snapshot_is_the_engine_s_rule_document() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    let r = hub.get(&format!("{}?id=DEP-01", route("rule"))).await;
    assert_eq!(r.status, 200, "{r:?}");
    assert_eq!(r.body, RULE.trim_end(), "passed on byte for byte");
    assert_eq!(r.json()["rule"]["text"], "Text of one.");
    assert_eq!(engine.params_of("rule"), [Some(json!({ "id": "DEP-01" }))]);
}

#[tokio::test]
async fn an_id_that_is_not_in_the_snapshot_never_reaches_the_engine() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    for id in [
        "NOPE-1",
        "dep-01",
        "DEP-01%20",
        "..%2F..%2Fetc%2Fpasswd",
        "%22%7D%2C%22x%22%3A%7B",
        "DEP-01%2Bx",
    ] {
        let r = hub.get(&format!("{}?id={id}", route("rule"))).await;
        assert_eq!(r.status, 404, "{id}: {r:?}");
        assert_eq!(r.json()["error"]["code"], "not_found", "{id}");
    }
    assert_eq!(
        engine.count("rule"),
        0,
        "no foreign string reached the engine"
    );
    assert_eq!(engine.count("snapshot"), 1);
}

#[tokio::test]
async fn a_query_that_is_no_id_is_refused_before_anything_is_asked() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    let long = "A".repeat(rules_hub::MAX_ID_BYTES + 1);
    for query in [
        String::new(),
        "?id=".to_owned(),
        "?x=DEP-01".to_owned(),
        "?id=DEP-01&id=DEP-09".to_owned(),
        "?id=DEP-01&x=1".to_owned(),
        "?id=%0ADEP-01".to_owned(),
        "?id=%FF".to_owned(),
        "?id=%G1".to_owned(),
        format!("?id={long}"),
    ] {
        let r = hub.get(&format!("{}{query}", route("rule"))).await;
        assert_eq!(r.status, 400, "{query}: {r:?}");
        assert_eq!(r.json()["error"]["code"], "bad_query", "{query}");
    }
    assert_eq!(engine.count("rule"), 0);
    assert_eq!(
        engine.count("snapshot"),
        0,
        "refused before the snapshot is read"
    );
}

#[tokio::test]
async fn a_rule_the_engine_no_longer_has_is_its_not_found() {
    // DEP-09 is in the snapshot; the fake engine has no document for it, as when the rule was
    // removed after the snapshot was read.
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    let r = hub.get(&format!("{}?id=DEP-09", route("rule"))).await;
    assert_eq!(r.status, 404, "{r:?}");
    assert_eq!(r.json()["error"]["code"], "not_found");
    assert_eq!(r.json()["error"]["message"], "no rule with the id DEP-09");
    assert_eq!(engine.count("rule"), 1);
}

#[tokio::test]
async fn the_ids_follow_the_snapshot_the_hub_holds() {
    let engine = FakeEngine::new();
    // A snapshot without DEP-01 first.
    let mut doc: Value = serde_json::from_str(SNAPSHOT).unwrap();
    doc["rules"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["id"] != "DEP-01");
    doc["digest"] = json!("sha256:1111111111111111");
    *engine.snapshot.lock().unwrap() = doc.to_string();
    // Every request reads the snapshot again.
    let hub = start_with(Some(Arc::clone(&engine)), Some(Duration::ZERO)).await;
    let path = format!("{}?id=DEP-01", route("rule"));
    assert_eq!(hub.get(&path).await.status, 404);
    assert_eq!(engine.count("rule"), 0);
    // The rule arrives in the next snapshot: now it is asked for.
    *engine.snapshot.lock().unwrap() = SNAPSHOT.trim_end().to_owned();
    let r = hub.get(&path).await;
    assert_eq!(r.status, 200, "{r:?}");
    assert_eq!(engine.count("rule"), 1);
}

#[tokio::test]
async fn a_snapshot_without_a_list_of_rules_asks_no_rule() {
    let engine = FakeEngine::new();
    *engine.snapshot.lock().unwrap() =
        r#"{"digest":"sha256:2222222222222222","kind":"rules.snapshot","schema":1}"#.to_owned();
    let hub = start(&engine).await;
    let r = hub.get(&format!("{}?id=DEP-01", route("rule"))).await;
    assert_eq!(r.status, 502, "{r:?}");
    assert_eq!(r.json()["error"]["code"], "engine_failed");
    assert_eq!(engine.count("rule"), 0);
}

#[tokio::test]
async fn without_an_engine_the_data_routes_say_so_and_about_still_answers() {
    let hub = start_with(None, None).await;
    for path in [
        route("snapshot"),
        format!("{}?id=DEP-01", route("rule")),
        route("sources"),
    ] {
        let r = hub.get(&path).await;
        assert_eq!(r.status, 503, "{path}: {r:?}");
        assert_eq!(r.json()["error"]["code"], "engine_unavailable");
        assert_eq!(r.json()["error"]["problem"], "engine_not_found");
    }
    let about = hub.get(&route("about")).await;
    assert_eq!(about.status, 200, "{about:?}");
    assert_eq!(about.json()["app"], "rules-desktop");
    assert!(about.json()["engine"].is_null());
    assert!(about.json()["engine_problem"].is_null());
}

#[tokio::test]
async fn about_names_the_engine_from_its_hello() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    let r = hub.get(&route("about")).await;
    assert_eq!(r.status, 200, "{r:?}");
    let doc = r.json();
    assert_eq!(doc["kind"], "hub.about");
    assert_eq!(doc["app"], "rules-desktop");
    assert_eq!(doc["version"], rules_hub::version());
    assert_eq!(
        doc["engine"],
        json!({ "version": "0.1.0", "git": "golden", "nvim": "golden" })
    );
    assert!(doc["engine_problem"].is_null());
    assert_eq!(engine.count("hello"), 1);

    // An engine that does not answer is told apart from no engine.
    *engine.hello.lock().unwrap() = Err(());
    let r = hub.get(&route("about")).await;
    assert_eq!(r.status, 200, "{r:?}");
    assert!(r.json()["engine"].is_null());
    assert_eq!(r.json()["engine_problem"], "engine_timeout");

    // A hello without an engine object is a broken engine, not a missing one.
    for hello in [
        r#"{"schema":1,"kind":"rules.hello"}"#,
        r#"{"engine":"text"}"#,
    ] {
        *engine.hello.lock().unwrap() = Ok(hello.to_owned());
        let r = hub.get(&route("about")).await;
        assert_eq!(r.status, 200, "{r:?}");
        assert!(r.json()["engine"].is_null(), "{hello}");
        assert_eq!(r.json()["engine_problem"], "engine_protocol", "{hello}");
    }
}

#[tokio::test]
async fn the_probe_accepts_the_golden_hello() {
    let engine = FakeEngine::new();
    let info = rules_hub::engine::probe(engine.as_ref()).await.unwrap();
    assert!(info.schemas.contains(&rules_hub::engine::SCHEMA));
    for needed in rules_hub::engine::REQUIRED_CAPABILITIES {
        assert!(info.capabilities.iter().any(|c| c == needed), "{needed}");
    }
}

fn policy(keys: &[(&str, Class)], default: Option<Class>) -> Arc<SourcePolicy> {
    Arc::new(
        SourcePolicy::new(keys.iter().map(|(k, c)| ((*k).to_owned(), *c)))
            .unwrap()
            .with_default(default),
    )
}

/// The golden snapshot with other files and one rule in each.
fn snapshot_with_files(paths: &[&str]) -> String {
    let mut doc: Value = serde_json::from_str(SNAPSHOT).unwrap();
    doc["sources"] = paths
        .iter()
        .map(|p| json!({ "etag": "sha256:0000000000000000", "path": p, "rules": 1 }))
        .collect();
    doc["digest"] = json!("sha256:3333333333333333");
    doc.to_string()
}

#[tokio::test]
async fn without_sources_json_every_file_is_unclassified() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    let r = hub.get(&route("sources")).await;
    assert_eq!(r.status, 200, "{r:?}");
    let doc = r.json();
    assert_eq!(doc["kind"], "hub.sources");
    assert_eq!(
        doc["files"],
        json!([
            { "path": "Checklists/DEP.md", "data_class": "unclassified", "rules": 2, "key": null },
            { "path": "Checklists/LUA.md", "data_class": "unclassified", "rules": 1, "key": null },
            { "path": "Checklists/_rules/ERR/ERR-100.md", "data_class": "unclassified",
              "rules": 1, "key": null },
        ])
    );
    assert_eq!(doc["keys"], json!([]));
    assert_eq!(doc["unclassified"].as_array().unwrap().len(), 3);
    assert_eq!(doc["private"], json!([]));
    // Computed from the held snapshot: one read for both answers, nothing else asked.
    assert_eq!(hub.get(&route("sources")).await.status, 200);
    assert_eq!(engine.count("snapshot"), 1);
    assert_eq!(engine.asked.lock().unwrap().len(), 1);

    let with_query = hub.get(&format!("{}?x=1", route("sources"))).await;
    assert_eq!(with_query.status, 400, "{with_query:?}");
}

#[tokio::test]
async fn sources_gives_each_file_its_class_and_the_strictest_of_two_keys() {
    let engine = FakeEngine::new();
    *engine.snapshot.lock().unwrap() = snapshot_with_files(&[
        "wkdbook-Lua/Checklists/DEP.md",
        "wkdbook-Lua/README.md",
        "Checklists#2/DEP.md",
        "lua/DEP.md",
        "native/REL.md",
    ]);
    let classes = policy(
        &[
            ("wkdbook-Lua", Class::Public),
            ("wkdbook-Lua/Checklists", Class::Private),
            ("Checklists", Class::Public),
            ("lua", Class::Internal),
        ],
        Some(Class::Internal),
    );
    let hub = start_classified(Some(Arc::clone(&engine)), None, classes).await;
    let r = hub.get(&route("sources")).await;
    assert_eq!(r.status, 200, "{r:?}");
    let doc = r.json();
    let shown: Vec<(&str, &str, Option<&str>)> = doc["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap(),
                f["data_class"].as_str().unwrap(),
                f["key"].as_str(),
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            // Two keys count, the strictest wins.
            (
                "wkdbook-Lua/Checklists/DEP.md",
                "private",
                Some("wkdbook-Lua/Checklists")
            ),
            ("wkdbook-Lua/README.md", "public", Some("wkdbook-Lua")),
            // A numbered name, not even the default.
            ("Checklists#2/DEP.md", "unclassified", None),
            ("lua/DEP.md", "internal", Some("lua")),
            ("native/REL.md", "internal", None),
        ]
    );
    assert_eq!(doc["unclassified"], json!(["Checklists#2/DEP.md"]));
    assert_eq!(doc["private"], json!(["wkdbook-Lua/Checklists/DEP.md"]));
    assert_eq!(
        doc["keys"],
        json!([
            { "key": "Checklists", "class": "public", "files": 1 },
            { "key": "lua", "class": "internal", "files": 1 },
            { "key": "wkdbook-Lua", "class": "public", "files": 2 },
            { "key": "wkdbook-Lua/Checklists", "class": "private", "files": 1 },
        ])
    );
}

/// The page's tests answer `sources` with `tests/fixtures/hub/sources.json` (ui/src/lib/fixtures.ts
/// imports it): it must be what the hub writes for the golden snapshot with the policy named there.
#[tokio::test]
async fn the_sources_document_of_the_page_tests_is_what_the_hub_writes() {
    let engine = FakeEngine::new();
    let classes = policy(
        &[
            ("Checklists", Class::Internal),
            ("Checklists/_rules", Class::Private),
        ],
        None,
    );
    let hub = start_classified(Some(Arc::clone(&engine)), None, classes).await;
    let r = hub.get(&route("sources")).await;
    assert_eq!(r.status, 200, "{r:?}");
    let written: Value = serde_json::from_str(HUB_SOURCES).unwrap();
    assert_eq!(r.json(), written, "the hub now writes {:#}", r.json());
}

#[tokio::test]
async fn a_snapshot_without_its_files_is_no_empty_list() {
    let engine = FakeEngine::new();
    let mut doc: Value = serde_json::from_str(SNAPSHOT).unwrap();
    doc.as_object_mut().unwrap().remove("sources");
    doc["digest"] = json!("sha256:4444444444444444");
    *engine.snapshot.lock().unwrap() = doc.to_string();
    let hub = start_classified(
        Some(Arc::clone(&engine)),
        None,
        policy(&[], Some(Class::Public)),
    )
    .await;
    let r = hub.get(&route("sources")).await;
    assert_eq!(r.status, 502, "{r:?}");
    assert_eq!(r.json()["error"]["code"], "engine_failed");
    // The rules of that snapshot are still read.
    let rule = hub.get(&format!("{}?id=DEP-01", route("rule"))).await;
    assert_eq!(rule.status, 200, "{rule:?}");
}

#[tokio::test]
async fn a_broken_file_list_costs_the_sources_route_but_not_the_rules() {
    let engine = FakeEngine::new();
    let mut doc: Value = serde_json::from_str(SNAPSHOT).unwrap();
    // A file without its count of rules.
    doc["sources"] = json!([{ "etag": "sha256:0000000000000000", "path": "Checklists/DEP.md" }]);
    doc["digest"] = json!("sha256:5555555555555555");
    *engine.snapshot.lock().unwrap() = doc.to_string();
    let hub = start_classified(
        Some(Arc::clone(&engine)),
        None,
        policy(&[], Some(Class::Public)),
    )
    .await;
    let r = hub.get(&route("sources")).await;
    assert_eq!(r.status, 502, "{r:?}");
    assert_eq!(r.json()["error"]["code"], "engine_failed");
    let rule = hub.get(&format!("{}?id=DEP-01", route("rule"))).await;
    assert_eq!(rule.status, 200, "{rule:?}");
}

/// Only what the loader refuses: no start reads `sources.json` yet, so that the start stops on
/// such a file is tested once the program's `serve` does.
#[test]
fn a_sources_json_with_a_hash_key_does_not_load() {
    let dir = std::env::temp_dir().join(format!("rules-hub-routes-hash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        rules_hub::sources::path(&dir),
        r#"{"schema":1,"sources":{"Checklists#2":{"class":"public"}}}"#,
    )
    .unwrap();
    assert!(SourcePolicy::load(&dir).is_err());
    std::fs::write(rules_hub::sources::path(&dir), "{").unwrap();
    assert!(SourcePolicy::load(&dir).is_err());
    std::fs::write(
        rules_hub::sources::path(&dir),
        r#"{"schema":1,"sources":{"Checklists":{"class":"public"}}}"#,
    )
    .unwrap();
    assert_eq!(SourcePolicy::load(&dir).unwrap().len(), 1);
}

/// The golden snapshot after a change: the file of `DEP-01` has a new etag, `DEP-09` is gone.
fn changed_snapshot(digest: &str) -> String {
    let mut doc: Value = serde_json::from_str(SNAPSHOT).unwrap();
    let rules = doc["rules"].as_array_mut().unwrap();
    rules.retain(|r| r["id"] != "DEP-09");
    rules[0]["etag"] = json!("sha256:6666666666666666");
    doc["rev"] = json!("rev:changed");
    doc["digest"] = json!(digest);
    doc.to_string()
}

/// Ask `search` until the index is no longer being built.
async fn search_when_built(hub: &Hub, query: &str) -> Value {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let r = hub.get(&format!("{}?{query}", route("search"))).await;
        assert_eq!(r.status, 200, "{r:?}");
        let doc = r.json();
        if doc["status"] != "building" {
            return doc;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the index was not built"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn the_search_finds_the_words_in_the_text_of_the_rules() {
    let engine = FakeEngine::hosted();
    let hub = start(&engine).await;
    let doc = search_when_built(&hub, "q=pin%20Dependency&limit=10").await;
    assert_eq!(doc["kind"], "hub.search");
    assert_eq!(doc["status"], "ready", "{doc}");
    assert_eq!(doc["indexed"], 4);
    assert_eq!(doc["partial"], false);
    assert_eq!(doc["total"], 1);
    assert_eq!(doc["hits"][0]["id"], "DEP-09");
    let snippet = &doc["hits"][0]["snippet"];
    assert_eq!(snippet["text"], "Text of nine: pin every dependency.");
    assert_eq!(snippet["marks"], json!([[14, 17], [24, 34]]));

    // A word in three texts, of which two are asked for.
    let doc = search_when_built(&hub, "q=text&limit=2").await;
    assert_eq!(doc["total"], 3, "{doc}");
    assert_eq!(doc["hits"].as_array().unwrap().len(), 2);

    // The text was read rule by rule; the engine was never asked for `bodies`, and the words
    // never reached it.
    assert_eq!(engine.count("bodies"), 0);
    let mut ids: Vec<String> = engine
        .params_of("rule")
        .into_iter()
        .map(|p| p.unwrap()["id"].as_str().unwrap().to_owned())
        .collect();
    ids.sort();
    assert_eq!(ids, ["DEP-01", "DEP-09", "ERR-100", "LUA-01"]);
    let asked = format!("{:?}", engine.asked.lock().unwrap());
    assert!(!asked.contains("ependency"), "{asked}");
}

#[tokio::test]
async fn without_a_running_host_the_search_fails_and_no_text_is_asked_for() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    let doc = search_when_built(&hub, "q=text").await;
    assert_eq!(doc["status"], "failed", "{doc}");
    assert_eq!(doc["hits"], json!([]));
    assert_eq!(engine.count("rule"), 0);
    assert_eq!(engine.count("snapshot"), 0);
}

#[tokio::test]
async fn a_search_needs_words_and_an_engine() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    for query in [
        "",
        "?q=a",
        "?q=two%20words&x=1",
        "?q=a+b",
        "?q=text&limit=0",
        "?q=text&limit=51",
        "?q=text&q=text",
        "?q=%0Atext",
    ] {
        let r = hub.get(&format!("{}{query}", route("search"))).await;
        assert_eq!(r.status, 400, "{query}: {r:?}");
        assert_eq!(r.json()["error"]["code"], "bad_query", "{query}");
    }
    let hub = start_with(None, None).await;
    let r = hub.get(&format!("{}?q=text", route("search"))).await;
    assert_eq!(r.status, 503, "{r:?}");
    assert_eq!(r.json()["error"]["code"], "engine_unavailable");
}

#[tokio::test]
async fn a_stream_says_where_it_starts_and_a_rescan_is_an_event_with_ids_only() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    let mut sse = Sse::open(&hub.server, Some(&hub.token)).await.unwrap();
    let hello = sse.next_event("hello", WAIT).await.expect("its hello");
    assert_eq!(hello["rev"], "rev:d62208");
    assert_eq!(hello["seq"], 0);
    assert_eq!(hello["instance"], hub.server.instance());

    *engine.snapshot.lock().unwrap() = changed_snapshot("sha256:7777777777777777");
    let r = hub.post(&route("rescan")).await;
    assert_eq!(r.status, 202, "{r:?}");
    let change = sse.next_event("rules", WAIT).await.expect("an event");
    assert_eq!(change["cause"], "rescan");
    assert_eq!(change["rev"], "rev:changed");
    assert_eq!(change["seq"], 1);
    assert_eq!(change["changed_ids"], json!(["DEP-01"]));
    assert_eq!(change["added"], json!([]));
    assert_eq!(change["removed"], json!(["DEP-09"]));
    assert_eq!(change["truncated"], false);
    // Ids only: no title, no text, no file.
    let text = change.to_string();
    assert!(!text.contains("nine") && !text.contains("DEP.md"), "{text}");
    // The held snapshot is the new one.
    let snap = hub.get(&route("snapshot")).await;
    let tag = format!("{:?}", "sha256:7777777777777777");
    assert_eq!(snap.header("etag"), Some(tag.as_str()));
}

#[tokio::test]
async fn a_stream_takes_no_query_and_without_an_engine_there_is_none() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    let r = hub.get(&format!("{}?since=3", route("events"))).await;
    assert_eq!(r.status, 400, "{r:?}");
    let hub = start_with(None, None).await;
    assert_eq!(
        Sse::open(&hub.server, Some(&hub.token)).await.err(),
        Some(503)
    );
    let r = hub.post(&route("rescan")).await;
    assert_eq!(r.status, 503, "{r:?}");
    assert_eq!(r.json()["error"]["code"], "engine_unavailable");
}

/// K22: a session that is closed takes its streams with it. The next event does not reach it.
#[tokio::test]
async fn a_stream_ends_with_its_session_before_it_is_told_another_id() {
    let engine = FakeEngine::new();
    let hub = Rig {
        engine: Some(Arc::clone(&engine)),
        max_sessions: 2,
        ..Rig::default()
    }
    .start()
    .await;
    let mut old = hub.stream().await;
    // The session of that stream is the oldest; two newer ones close it.
    let _second = session(&hub.server, &hub.control).await;
    let third = session(&hub.server, &hub.control).await;
    let mut open = Sse::open(&hub.server, Some(&third)).await.unwrap();
    assert!(open.next_event("hello", WAIT).await.is_some());

    *engine.snapshot.lock().unwrap() = changed_snapshot("sha256:8888888888888888");
    let r = post(&hub.server, &route("rescan"), Some(&third), "{}").await;
    assert_eq!(r.status, 202, "{r:?}");
    // The open session is told.
    let change = open.next_event("rules", WAIT).await.expect("an event");
    assert_eq!(change["removed"], json!(["DEP-09"]));
    // The closed one is not: its stream ends instead.
    let (ended, seen) = old.ended(WAIT).await;
    assert!(ended, "the stream outlived its session: {seen:?}");
    assert!(
        seen.iter().all(|m| !m.contains("event: rules")),
        "an id went to a closed session: {seen:?}"
    );
}

/// K22: the session is checked before the hello too. It may be closed while the snapshot the
/// stream starts from is read.
#[tokio::test]
async fn a_stream_whose_session_is_closed_while_it_opens_is_not_greeted() {
    let engine = FakeEngine::new();
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    *engine.snapshot_gate.lock().unwrap() = Some(Arc::clone(&gate));
    let hub = Rig {
        engine: Some(Arc::clone(&engine)),
        max_sessions: 2,
        ..Rig::default()
    }
    .start()
    .await;
    let opening = Sse::open(&hub.server, Some(&hub.token));
    let closing = async {
        let deadline = tokio::time::Instant::now() + WAIT;
        while engine.count("snapshot") == 0 {
            assert!(tokio::time::Instant::now() < deadline, "no snapshot asked");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // Two newer sessions close the one of the stream while it waits for the snapshot.
        session(&hub.server, &hub.control).await;
        session(&hub.server, &hub.control).await;
        gate.add_permits(1000);
    };
    let (old, ()) = tokio::join!(opening, closing);
    let mut old = old.expect("the stream opens");
    let (ended, seen) = old.ended(WAIT).await;
    assert!(ended, "the stream outlived its session: {seen:?}");
    assert!(
        seen.iter().all(|m| !m.contains("event: hello")),
        "a closed session was greeted: {seen:?}"
    );
}

/// A new folder for a vault of one rule set.
fn temp_vault(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rules-hub-routes-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos())
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("rulesets").join("Fixture").join(".rules-ops")).unwrap();
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    dir
}

#[tokio::test]
async fn a_change_of_a_rule_file_wakes_a_scan_and_a_leftover_does_not() {
    let engine = FakeEngine::new();
    let vault = temp_vault("watch");
    let hub = Rig {
        engine: Some(Arc::clone(&engine)),
        vault: Some(vault.clone()),
        ..Rig::default()
    }
    .start()
    .await;
    let mut sse = hub.stream().await;
    let set = vault.join("rulesets").join("Fixture");

    // What tools leave and what the engine keeps for itself wakes nothing.
    *engine.snapshot.lock().unwrap() = changed_snapshot("sha256:9999999999999999");
    std::fs::write(set.join("DEP.md~"), "x").unwrap();
    std::fs::write(set.join(".DEP.md.swp"), "x").unwrap();
    std::fs::write(set.join("4913"), "x").unwrap();
    std::fs::write(set.join("notes.txt"), "x").unwrap();
    std::fs::write(set.join(".rules-ops").join("op.md"), "x").unwrap();
    std::fs::write(vault.join(".git").join("HEAD"), "x").unwrap();
    assert!(
        sse.next_event("rules", Duration::from_millis(1500))
            .await
            .is_none(),
        "a leftover woke a scan"
    );
    let before = engine.count("snapshot");

    // A rule file does.
    std::fs::write(set.join("DEP.md"), "# Dependencies\n").unwrap();
    let change = sse.next_event("rules", WAIT).await.expect("an event");
    assert_eq!(change["cause"], "fs");
    assert_eq!(change["changed_ids"], json!(["DEP-01"]));
    assert_eq!(engine.count("snapshot"), before + 1, "one scan");
    drop(hub);
    let _ = std::fs::remove_dir_all(&vault);
}
