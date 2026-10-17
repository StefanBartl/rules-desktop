//! The read routes of the hub against an engine in the process, without Neovim: a fake that answers
//! with the golden documents of rules.nvim (`tests/fixtures/golden`, a copy of
//! `schemas/rules-export/1/golden` at the pinned commit; CI compares the two) and counts what it
//! was asked.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use appkit_core::authn::{ManualClock, Secret, CODE_BYTES, TOKEN_BYTES};
use appkit_hub::engine::{CallFuture, Engine, EngineError, EngineRefusal, Method, Reply};
use appkit_hub::{HubStatus, Limits, RunningServer};
use rules_hub::sources::{Class, SourcePolicy};
use rules_hub::Watch;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod common;

use common::{get, get_with, post, session};

const HELLO: &str = include_str!("fixtures/golden/hello.json");
const SNAPSHOT: &str = include_str!("fixtures/golden/snapshot.json");
const RULE: &str = include_str!("fixtures/golden/rule.json");
const ERROR: &str = include_str!("fixtures/golden/error.json");
/// What the hub answers `sources` with for the golden snapshot; the page's tests answer with it.
const HUB_SOURCES: &str = include_str!("fixtures/hub/sources.json");

/// The digest of the golden snapshot: its ETag.
const SNAPSHOT_DIGEST: &str = "sha256:9609c01610189534";

/// An engine in the process. It answers `hello` and `snapshot` with the golden documents, `rule`
/// with the golden rule for its id (`DEP-01`) and a `not_found` refusal for any other, and keeps
/// every question.
struct FakeEngine {
    snapshot: Mutex<String>,
    hello: Mutex<Result<String, ()>>,
    asked: Mutex<Vec<(&'static str, Option<Value>)>>,
}

impl FakeEngine {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            snapshot: Mutex::new(SNAPSHOT.trim_end().to_owned()),
            hello: Mutex::new(Ok(HELLO.trim_end().to_owned())),
            asked: Mutex::new(Vec::new()),
        })
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
                "snapshot" => self.snapshot.lock().unwrap().clone(),
                "rule" => {
                    let id = params.and_then(|p| p["id"].as_str()).unwrap_or_default();
                    if id != "DEP-01" {
                        return Err(Self::not_found(id));
                    }
                    RULE.trim_end().to_owned()
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

struct Hub {
    server: RunningServer,
    token: String,
}

fn ready() -> HubStatus {
    HubStatus {
        engine_ready: true,
        problem: None,
        capabilities: vec!["hello".into(), "snapshot".into(), "rule".into()],
    }
}

async fn start_with(engine: Option<Arc<FakeEngine>>, max_age: Option<Duration>) -> Hub {
    start_classified(engine, max_age, Arc::default(), None).await
}

async fn start_classified(
    engine: Option<Arc<FakeEngine>>,
    max_age: Option<Duration>,
    sources: Arc<SourcePolicy>,
    watch: Option<Watch>,
) -> Hub {
    let control = Secret::generate(TOKEN_BYTES).unwrap();
    let server = rules_hub::start(rules_hub::Start {
        addr: "127.0.0.1:0".parse().unwrap(),
        limits: Limits {
            snapshot_max_age: max_age,
            ..Limits::default()
        },
        clock: Arc::new(ManualClock::new()),
        pairing_ttl: Duration::from_secs(60),
        control_secret: control.clone(),
        instance: Secret::generate(CODE_BYTES).unwrap(),
        status: if engine.is_some() {
            ready()
        } else {
            HubStatus {
                engine_ready: false,
                problem: Some("engine_not_found"),
                capabilities: Vec::new(),
            }
        },
        engine: engine.map(|e| e as Arc<dyn Engine>),
        sources,
        watch,
        ui: rules_hub::ui::UiFiles::none(),
    })
    .await
    .expect("the hub starts");
    let token = session(&server, &control).await;
    Hub { server, token }
}

async fn start(engine: &Arc<FakeEngine>) -> Hub {
    // The snapshot is held until something says it changed: each fetch is one the test caused.
    start_with(Some(Arc::clone(engine)), None).await
}

fn route(name: &str) -> String {
    rules_hub::IDENTITY.route(name)
}

impl Hub {
    async fn get(&self, path: &str) -> appkit_testkit::wire::Reply {
        get(&self.server, path, Some(&self.token)).await
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
        // The stream does not end and the rescan is a POST: both are tried on their own below.
        if matches!(*name, "events" | "rescan") {
            continue;
        }
        let with = hub.get(&route(name)).await;
        // Registered: a route that is not would be a 404 of the router with an empty body.
        assert!(with.status == 200 || with.status == 400, "{name}: {with:?}");
        seen.insert(*name, with.status);
    }
    assert_eq!(seen.get("snapshot"), Some(&200));
    // `rule` without an id is a bad query, told by the route.
    assert_eq!(seen.get("rule"), Some(&400));
    assert_eq!(seen.get("sources"), Some(&200));
    assert_eq!(seen.get("about"), Some(&200));
    // `search` without a query is a bad query, told by the route.
    assert_eq!(seen.get("search"), Some(&400));
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
    let hub = start_classified(Some(Arc::clone(&engine)), None, classes, None).await;
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
    let hub = start_classified(Some(Arc::clone(&engine)), None, classes, None).await;
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
        None,
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
        None,
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

// The live routes: the event stream, the rescan and the search.

fn quick() -> rules_hub::store::RefreshOptions {
    rules_hub::store::RefreshOptions {
        debounce: Duration::from_millis(50),
        max_wait: Duration::from_millis(300),
        interval: Duration::from_secs(600),
    }
}

/// A folder to watch, shaped like a vault.
fn watched(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("rules-desktop-live-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("rulesets").join("alpha")).unwrap();
    dir
}

async fn start_watching(engine: &Arc<FakeEngine>, dir: &std::path::Path) -> Hub {
    start_classified(
        Some(Arc::clone(engine)),
        None,
        Arc::default(),
        Some(Watch {
            vault: dir.to_path_buf(),
            refresh: quick(),
        }),
    )
    .await
}

/// The golden snapshot with another etag for DEP-01 and another digest: a change.
fn changed_snapshot() -> String {
    let mut doc: Value = serde_json::from_str(SNAPSHOT).unwrap();
    doc["digest"] = json!("sha256:7777777777777777");
    doc["rev"] = json!("rev:777777");
    let rules = doc["rules"].as_array_mut().unwrap();
    let dep = rules.iter_mut().find(|r| r["id"] == "DEP-01").unwrap();
    dep["etag"] = json!("sha256:1212121212121212");
    doc.to_string()
}

/// An open event stream, read as it comes.
struct Stream {
    socket: tokio::net::TcpStream,
    seen: String,
}

impl Stream {
    async fn open(hub: &Hub) -> Stream {
        let mut socket = tokio::net::TcpStream::connect(hub.server.addr())
            .await
            .unwrap();
        let host = format!("127.0.0.1:{}", hub.server.addr().port());
        let bearer = format!("Bearer {}", hub.token);
        let bytes = appkit_testkit::wire::request(
            "GET",
            &route("events"),
            &[("Host", host.as_str()), ("Authorization", bearer.as_str())],
            b"",
        );
        socket.write_all(&bytes).await.unwrap();
        Stream {
            socket,
            seen: String::new(),
        }
    }

    /// Read until the text holds `needle`; the whole text read so far.
    async fn until(&mut self, needle: &str) -> String {
        let end = std::time::Instant::now() + Duration::from_secs(15);
        while !self.seen.contains(needle) {
            let left = end.saturating_duration_since(std::time::Instant::now());
            let mut buf = [0u8; 4096];
            let n = tokio::time::timeout(left, self.socket.read(&mut buf))
                .await
                .unwrap_or_else(|_| panic!("no {needle:?} in {:?}", self.seen))
                .unwrap();
            assert!(n > 0, "the stream ended before {needle:?}: {:?}", self.seen);
            self.seen.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        self.seen.clone()
    }
}

#[tokio::test]
async fn the_stream_says_hello_and_then_what_the_watcher_saw() {
    let engine = FakeEngine::new();
    let dir = watched("fs");
    let hub = start_watching(&engine, &dir).await;
    let mut stream = Stream::open(&hub).await;
    let hello = stream.until("event: hello").await;
    assert!(hello.starts_with("HTTP/1.1 200"), "{hello}");
    assert!(hello.contains("text/event-stream"), "{hello}");
    stream.until("\n\n").await;

    // A rule file changes on disk, and the engine now says something else about it.
    *engine.snapshot.lock().unwrap() = changed_snapshot();
    std::fs::write(dir.join("rulesets/alpha/new.md"), "# x\n").unwrap();
    let text = stream.until("event: rules").await;
    assert!(text.contains("DEP-01"), "{text}");
    assert!(text.contains(r#""cause":"fs""#), "{text}");
}

#[tokio::test]
async fn a_file_that_is_no_rule_wakes_nothing() {
    let engine = FakeEngine::new();
    let dir = watched("quiet");
    let hub = start_watching(&engine, &dir).await;
    let mut stream = Stream::open(&hub).await;
    stream.until("event: hello").await;
    let before = engine.count("snapshot");
    *engine.snapshot.lock().unwrap() = changed_snapshot();
    std::fs::write(dir.join("README.md"), "x").unwrap();
    std::fs::write(dir.join("rulesets/alpha/.new.md.swp"), "x").unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(engine.count("snapshot"), before, "a scan was woken");
    assert!(!stream.seen.contains("event: rules"), "{}", stream.seen);
}

#[tokio::test]
async fn a_rescan_is_accepted_and_the_stream_tells_what_it_found() {
    let engine = FakeEngine::new();
    let dir = watched("rescan");
    let hub = start_watching(&engine, &dir).await;
    let mut stream = Stream::open(&hub).await;
    stream.until("event: hello").await;

    let anonymous = post(&hub.server, &route("rescan"), None, "{}").await;
    assert_eq!(anonymous.status, 401, "{anonymous:?}");

    *engine.snapshot.lock().unwrap() = changed_snapshot();
    let r = post(&hub.server, &route("rescan"), Some(&hub.token), "{}").await;
    assert_eq!(r.status, 202, "{r:?}");
    let text = stream.until("event: rules").await;
    assert!(text.contains(r#""cause":"rescan""#), "{text}");
}

#[tokio::test]
async fn a_session_has_a_few_streams_and_no_more() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    let mut open = Vec::new();
    for _ in 0..4 {
        let mut s = Stream::open(&hub).await;
        s.until("event: hello").await;
        open.push(s);
    }
    let mut fifth = Stream::open(&hub).await;
    let said = fifth.until("\r\n\r\n").await;
    assert!(said.starts_with("HTTP/1.1 429"), "{said}");
}

#[tokio::test]
async fn without_an_engine_there_is_no_stream_no_search_and_no_rescan() {
    let hub = start_with(None, None).await;
    for name in ["events", "search"] {
        let path = if name == "search" {
            format!("{}?q=text", route(name))
        } else {
            route(name)
        };
        let r = hub.get(&path).await;
        assert_eq!(r.status, 503, "{name}: {r:?}");
    }
    let r = post(&hub.server, &route("rescan"), Some(&hub.token), "{}").await;
    assert_eq!(r.status, 503, "{r:?}");
}

#[tokio::test]
async fn the_search_finds_the_rules_whose_title_or_text_hold_the_words() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    let path = format!("{}?q=text%20of", route("search"));
    let mut answer = Value::Null;
    for _ in 0..100 {
        let r = hub.get(&path).await;
        assert_eq!(r.status, 200, "{r:?}");
        answer = r.json();
        if answer["status"] == "ready" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(answer["kind"], "hub.search");
    assert_eq!(answer["status"], "ready", "{answer}");
    // The fake knows the text of DEP-01 only ("Text of one."); the other rules are not found by
    // the engine and are left out of the index.
    let ids: Vec<&str> = answer["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["DEP-01"], "{answer}");
    assert!(answer["hits"][0]["snippet"]["text"]
        .as_str()
        .unwrap()
        .contains("Text of one"));
    // The title counts, too.
    let by_title = hub.get(&format!("{}?q=one", route("search"))).await.json();
    assert_eq!(by_title["hits"][0]["id"], "DEP-01");
    // `bodies` is not a route: the kit's call is the hub's own.
    let r = hub.get(&route("bodies")).await;
    assert_eq!(r.status, 404, "{r:?}");
}

#[tokio::test]
async fn a_bad_search_is_refused_before_the_index_is_asked() {
    let engine = FakeEngine::new();
    let hub = start(&engine).await;
    for bad in ["", "?q=", "?q=a", "?x=1", "?q=text&q=more", "?q=%ZZ"] {
        let r = hub.get(&format!("{}{bad}", route("search"))).await;
        assert_eq!(r.status, 400, "{bad:?}: {r:?}");
    }
}

#[tokio::test]
async fn the_page_of_a_folder_is_served_to_everybody_and_nothing_else_of_it() {
    let dir = std::env::temp_dir().join(format!("rules-desktop-page-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("index.html"), "<!doctype html><title>x</title>").unwrap();
    std::fs::write(dir.join("assets").join("app-1.js"), "export {};").unwrap();
    std::fs::write(dir.join("secret.html"), "no").unwrap();
    let control = Secret::generate(TOKEN_BYTES).unwrap();
    let server = rules_hub::start(rules_hub::Start {
        addr: "127.0.0.1:0".parse().unwrap(),
        limits: Limits::default(),
        clock: Arc::new(ManualClock::new()),
        pairing_ttl: Duration::from_secs(60),
        control_secret: control,
        instance: Secret::generate(CODE_BYTES).unwrap(),
        status: ready(),
        engine: None,
        sources: Arc::default(),
        watch: None,
        ui: rules_hub::ui::UiFiles::load(&dir).unwrap(),
    })
    .await
    .unwrap();
    let page = get(&server, "/", None).await;
    assert_eq!(page.status, 200, "{page:?}");
    assert!(page.body.contains("<title>x</title>"));
    assert_eq!(get(&server, "/assets/app-1.js", None).await.status, 200);
    // Only what the build holds: a file next to it is not there.
    assert_eq!(get(&server, "/secret.html", None).await.status, 404);
    let csp = page.header("content-security-policy").unwrap_or_default();
    assert!(csp.contains("default-src 'none'"), "{csp}");
    assert!(
        csp.contains("script-src 'self'") && !csp.contains("unsafe"),
        "{csp}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
