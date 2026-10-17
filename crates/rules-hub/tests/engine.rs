//! The hub against the real engine: rules.nvim and lib.nvim at the commits of `engine.lock`, through
//! its persistent host, on a copy of the fixture vault `tests/fixtures/vault` (one rule set
//! `Fixture`: two rules in one document, one with a `grep` check; a rule in a file of its own; a
//! retired rule; and `LUA-01`, whose check is a function that writes a marker file when it runs).
//!
//! The test needs Neovim >= 0.12 and `RULES_ENGINE_DIR` naming a rules.nvim checkout (lib.nvim is
//! found as the hub finds it, `LIB_NVIM_DIR` first). Without them it says so and returns; with
//! `RULES_DESKTOP_REQUIRE_ENGINE=1` (as CI sets it) a missing engine is a failure instead.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use appkit_core::authn::{ManualClock, Secret, CODE_BYTES, TOKEN_BYTES};
use appkit_hub::config::FileConfig;
use appkit_hub::discovery::{discover, AppInputs, Setup, SetupProblem};
use appkit_hub::env::Env;
use appkit_hub::{HubStatus, Limits};
use rules_hub::bodies::HostedEngine;
use rules_hub::engine::{cli_config, probe, CliEngine, DISCOVERY};
use rules_hub::store::RefreshOptions;
use serde_json::Value;

mod common;

use common::{get, session, Sse};

const REQUIRE: &str = "RULES_DESKTOP_REQUIRE_ENGINE";
const WAIT: Duration = Duration::from_secs(60);

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn skip(why: &str) {
    assert!(
        std::env::var_os(REQUIRE).is_none(),
        "the engine is required but not usable: {why}"
    );
    eprintln!("skipped: {why}");
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// A fresh copy of the fixture vault, with the marker of the canary inside the copy's folder.
struct Vault {
    root: PathBuf,
    vault: PathBuf,
    marker: PathBuf,
}

impl Vault {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "rules-hub-engine-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos())
        ));
        let _ = std::fs::remove_dir_all(&root);
        let vault = root.join("vault");
        copy_dir(&fixtures().join("vault"), &vault);
        let marker = root.join("a-check-ran.txt");
        let lua = vault.join("rulesets").join("Fixture").join("LUA.md");
        let text = std::fs::read_to_string(&lua).unwrap();
        // A Lua string: forward slashes are a path on every system.
        let path = marker.display().to_string().replace('\\', "/");
        assert!(text.contains("@MARKER@"));
        std::fs::write(&lua, text.replace("@MARKER@", &path)).unwrap();
        Self {
            root,
            vault,
            marker,
        }
    }

    fn set(&self) -> PathBuf {
        self.vault.join("rulesets").join("Fixture")
    }
}

impl Drop for Vault {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// What the hub would find for this vault, or `None` (and the reason told) without an engine.
async fn setup(vault: &Path) -> Option<Setup> {
    let env = Env::from_process();
    if env.path(DISCOVERY.engine_dir_var).is_none() {
        skip("RULES_ENGINE_DIR is not set");
        return None;
    }
    let file: FileConfig = FileConfig {
        vault: Some(vault.to_path_buf()),
        ..FileConfig::default()
    };
    let inputs = AppInputs {
        lib_dir: None,
        extra_areas: &[],
    };
    match discover(&DISCOVERY, &file, inputs, &env).await {
        Ok(setup) if setup.lib_dir.is_none() => {
            skip("no lib.nvim checkout was found (set LIB_NVIM_DIR)");
            None
        }
        Ok(setup) => Some(setup),
        Err(
            problem @ (SetupProblem::NvimNotFound { .. }
            | SetupProblem::NvimTooOld { .. }
            | SetupProblem::NvimProbeFailed(_)
            | SetupProblem::EngineNotFound { .. }
            | SetupProblem::LibNotFound { .. }),
        ) => {
            skip(&problem.to_string());
            None
        }
        Err(other) => panic!("the fixture vault is not usable: {other}"),
    }
}

async fn search(server: &appkit_hub::RunningServer, token: &str, q: &str) -> Value {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let r = get(server, &format!("/api/rules/search?q={q}"), Some(token)).await;
        assert_eq!(r.status, 200, "{r:?}");
        let doc = r.json();
        if doc["status"] == "ready" {
            return doc;
        }
        assert_ne!(doc["status"], "failed", "the index was not built: {doc}");
        assert!(tokio::time::Instant::now() < deadline, "no index: {doc}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::test]
async fn the_hub_reads_watches_and_searches_the_real_engine_and_never_runs_a_check() {
    let fixture = Vault::new();
    let Some(setup) = setup(&fixture.vault).await else {
        return;
    };
    let engine = Arc::new(CliEngine::with_host(cli_config(
        &setup,
        Env::from_process(),
    )));
    let info = probe(engine.as_ref())
        .await
        .expect("the engine answers hello");
    assert!(info.schemas.contains(&rules_hub::engine::SCHEMA));
    // The search reads the text through the host: it must be the one that answered.
    assert!(
        engine.host_running().await,
        "the engine has no running host"
    );

    let control = Secret::generate(TOKEN_BYTES).unwrap();
    let server = rules_hub::start(rules_hub::Start {
        addr: "127.0.0.1:0".parse().unwrap(),
        limits: Limits {
            snapshot_max_age: None,
            ..Limits::default()
        },
        clock: Arc::new(ManualClock::new()),
        pairing_ttl: Duration::from_secs(60),
        control_secret: control.clone(),
        instance: Secret::generate(CODE_BYTES).unwrap(),
        status: HubStatus {
            engine_ready: true,
            problem: None,
            capabilities: info.capabilities.clone(),
        },
        engine: Some(Arc::clone(&engine) as Arc<dyn HostedEngine>),
        sources: Arc::default(),
        vault: Some(fixture.vault.clone()),
        refresh: RefreshOptions {
            debounce: Duration::from_millis(200),
            max_wait: Duration::from_secs(1),
            interval: Duration::from_secs(300),
        },
    })
    .await
    .expect("the hub starts");
    let token = session(&server, &control).await;

    // The snapshot holds the rule set of the copied vault and nothing else of this machine.
    let r = get(&server, "/api/rules/snapshot", Some(&token)).await;
    assert_eq!(r.status, 200, "{r:?}");
    let snap = r.json();
    assert_eq!(snap["kind"], "rules.snapshot");
    assert_eq!(snap["load_errors"], serde_json::json!([]), "{snap}");
    let ids: Vec<&str> = snap["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["DEP-01", "DEP-02", "ERR-100", "LUA-01"]);
    for rule in snap["rules"].as_array().unwrap() {
        assert!(
            rule["source"].as_str().unwrap().starts_with("Fixture/"),
            "another rule set came in: {rule}"
        );
    }
    assert_eq!(snap["retired"][0]["id"], "ERR-007");
    let shown = fixture.root.display().to_string().replace('\\', "/");
    assert!(
        !r.body.replace("\\\\", "/").contains(&shown),
        "a path of this machine in the snapshot"
    );

    // The rule whose check is a function is shown, never offered for editing.
    let r = get(&server, "/api/rules/rule?id=LUA-01", Some(&token)).await;
    assert_eq!(r.status, 200, "{r:?}");
    let lua = r.json();
    assert_eq!(lua["rule"]["editable"], false, "{lua}");
    assert_eq!(lua["rule"]["check_type"], "lua_predicate");
    let r = get(&server, "/api/rules/rule?id=DEP-01", Some(&token)).await;
    assert_eq!(r.status, 200, "{r:?}");
    assert_eq!(r.json()["rule"]["editable"], true);
    assert!(
        r.json()["rule"]["text"]
            .as_str()
            .unwrap()
            .contains("pinned to an exact version"),
        "{}",
        r.body
    );

    // The text of every rule is searched, read through the host.
    let found = search(&server, &token, "wildcard").await;
    assert_eq!(found["indexed"], 4, "{found}");
    assert_eq!(found["total"], 1, "{found}");
    assert_eq!(found["hits"][0]["id"], "DEP-02");

    // A change of a rule file is an event with the ids of that file, and the index follows it.
    let mut sse = Sse::open(&server, Some(&token)).await.expect("a stream");
    assert!(sse.next_event("hello", WAIT).await.is_some());
    let dep = fixture.set().join("DEP.md");
    let text = std::fs::read_to_string(&dep).unwrap();
    std::fs::write(
        &dep,
        text.replace(
            "never ends in a wildcard.",
            "never ends in a wildcard or a caret.",
        ),
    )
    .unwrap();
    let change = loop {
        let change = sse.next_event("rules", WAIT).await.expect("an event");
        // A first scan may still see the old text (the engine does not trust a file changed
        // within two seconds, and an editor may write twice): the event with the ids counts.
        if !change["changed_ids"].as_array().unwrap().is_empty() {
            break change;
        }
    };
    assert_eq!(change["cause"], "fs", "{change}");
    assert_eq!(
        change["changed_ids"],
        serde_json::json!(["DEP-01", "DEP-02"]),
        "the ids follow the etag of the file"
    );
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let doc = search(&server, &token, "caret").await;
        if doc["total"] == 1 {
            assert_eq!(doc["hits"][0]["id"], "DEP-02");
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "{doc}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // After the snapshot, the rules, the index and its update: no check ran.
    assert!(!fixture.marker.exists(), "the hub ran the check of LUA-01");
    server.shutdown().await;
    engine.stop_host().await;

    // And the canary is live: the engine's own runner does write the marker.
    let lib = setup.lib_dir.as_ref().unwrap().value.clone();
    let out = std::process::Command::new(&setup.nvim.value)
        .args(["--headless", "-u", "NONE", "-i", "NONE", "-n", "-l"])
        .arg(fixtures().join("run-check.lua"))
        .arg(&setup.engine_dir.value)
        .arg(&lib)
        .arg(fixture.set())
        .arg("LUA")
        .output()
        .expect("nvim runs");
    assert!(out.status.success(), "{out:?}");
    assert!(
        fixture.marker.exists(),
        "the canary did not write its marker when checked: {out:?}"
    );
}
