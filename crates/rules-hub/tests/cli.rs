//! The program as a person runs it: arguments in, text and exit status out; and `serve` with the real
//! engine, a session and the data classes of a config folder.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use rules_hub::client;
use serde_json::Value;

fn fixture_vault() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("vault")
}

fn config_dir(name: &str, sources: Option<&str>) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rules-desktop-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    if let Some(text) = sources {
        std::fs::write(dir.join("sources.json"), text).unwrap();
    }
    dir
}

/// The program with a config folder of its own and the fixture vault. Only what a program needs to
/// run and to find Neovim and the engine is handed on; no variable of the person who runs the test
/// that begins with `RULES_` reaches it.
fn command(config: &Path, args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_rules-hub"));
    c.args(args).env_clear();
    for (k, v) in std::env::vars_os() {
        let name = k.to_string_lossy().to_ascii_uppercase();
        if !name.starts_with("RULES_") && !name.starts_with("TASKS_") {
            c.env(k, v);
        }
    }
    c.env("RULES_DESKTOP_CONFIG_DIR", config)
        .env("RULES_VAULT", fixture_vault());
    if let Some(engine) = std::env::var_os("RULES_ENGINE_DIR") {
        c.env("RULES_ENGINE_DIR", engine);
    }
    c
}

/// The program to its end. One that has not ended after a minute is a hub that was meant to refuse
/// and runs: it is killed and the test fails, instead of waiting for ever.
fn run(config: &Path, args: &[&str]) -> Output {
    let mut child = command(config, args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the program starts");
    let until = Instant::now() + Duration::from_secs(60);
    while child.try_wait().expect("wait").is_none() {
        if Instant::now() > until {
            let _ = child.kill();
            panic!("{args:?} did not end");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    child.wait_with_output().expect("output")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn the_version_is_the_name_and_the_number() {
    let out = run(&config_dir("version", None), &["--version"]);
    assert!(out.status.success());
    assert_eq!(text(&out.stdout).trim(), rules_hub::version_line());
}

#[test]
fn an_unknown_verb_prints_the_usage_and_fails() {
    let out = run(&config_dir("unknown", None), &["frobnicate"]);
    assert_eq!(out.status.code(), Some(1));
    let err = text(&out.stderr);
    for verb in [
        "serve",
        "start",
        "stop",
        "open",
        "autostart",
        "install",
        "uninstall",
    ] {
        assert!(err.contains(&format!("rules-hub {verb}")), "{verb}: {err}");
    }
    // The usage names this program and not the tasks hub.
    assert!(!err.contains("tasks"), "{err}");
}

#[test]
fn a_bad_port_or_a_stray_argument_is_refused_before_anything_starts() {
    let dir = config_dir("badargs", None);
    for args in [
        &["serve", "--port", "80"][..],
        &["serve", "--port", "x"],
        &["serve", "--bogus"],
        &["stop", "now"],
        &["autostart", "maybe"],
        &["install", "--bogus"],
        &["uninstall", "--bogus"],
        &["open", "--bogus"],
    ] {
        let out = run(&dir, args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
    }
}

#[test]
fn open_without_a_running_hub_says_so() {
    let out = run(&config_dir("norun", None), &["open"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("no hub is running"));
}

#[test]
fn a_sources_file_that_cannot_be_used_stops_the_start() {
    let dir = config_dir(
        "badsources",
        Some(r#"{"schema":1,"sources":{"alpha":{"class":"publik"}}}"#),
    );
    let out = run(&dir, &["serve", "--port", "40000"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("sources.json"),
        "{}",
        text(&out.stderr)
    );
    assert!(!dir.join("hub.json").exists());
}

/// A hub that runs in a child process, stopped when the test ends however it ends.
struct Running {
    child: Child,
    dir: PathBuf,
    port: u16,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

fn serve(name: &str, sources: Option<&str>) -> Running {
    let dir = config_dir(name, sources);
    let port = free_port();
    let child = command(&dir, &["serve", "--port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the hub starts");
    let running = Running { child, dir, port };
    let until = Instant::now() + Duration::from_secs(30);
    while !running.dir.join("hub.json").exists() {
        assert!(Instant::now() < until, "the hub did not announce itself");
        std::thread::sleep(Duration::from_millis(100));
    }
    running
}

async fn token(running: &Running) -> String {
    let out = run(&running.dir, &["open"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let link = text(&out.stdout);
    let code = link.trim().split_once("#pair=").expect("a pairing link").1;
    let body = format!(r#"{{"code":"{code}"}}"#);
    // The page exchanges the code, so the request carries the origin of the page.
    let origin = format!("http://127.0.0.1:{}", running.port);
    let reply = client::request(
        running.port,
        "POST",
        "/auth/exchange",
        &[("Content-Type", "application/json"), ("Origin", &origin)],
        &body,
    )
    .await
    .expect("exchange");
    assert_eq!(reply.status, 200, "{}", reply.body);
    let doc: Value = serde_json::from_str(&reply.body).unwrap();
    doc["token"].as_str().unwrap().to_owned()
}

async fn get(running: &Running, token: &str, path: &str) -> (u16, Value) {
    let auth = format!("Bearer {token}");
    let reply = client::request(running.port, "GET", path, &[("Authorization", &auth)], "")
        .await
        .expect("request");
    let doc = serde_json::from_str(&reply.body).unwrap_or(Value::Null);
    (reply.status, doc)
}

const CLASSES: &str = r#"{"schema":1,"sources":{
    "alpha":{"class":"public"},"beta":{"class":"internal"},"secret":{"class":"private"}}}"#;

#[tokio::test]
async fn serve_reads_the_fixture_rules_with_the_classes_of_the_config_folder() {
    let hub = serve("serve", Some(CLASSES));
    // Nobody gets in without the pairing: the data routes need a session.
    let anon = client::request(hub.port, "GET", "/api/rules/snapshot", &[], "")
        .await
        .expect("request");
    assert_eq!(anon.status, 401);

    let token = token(&hub).await;
    let (status, hello) = get(&hub, &token, "/api/rules/hello").await;
    assert_eq!(status, 200);
    assert_eq!(hello["engine"]["ready"], true, "{hello}");

    let (status, snap) = get(&hub, &token, "/api/rules/snapshot").await;
    assert_eq!(status, 200, "{snap}");
    let ids: Vec<&str> = snap["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["AA-01", "AA-02", "AA-03", "BB-01", "BB-02", "SS-01"]);

    let (status, sources) = get(&hub, &token, "/api/rules/sources").await;
    assert_eq!(status, 200, "{sources}");
    let class_of = |prefix: &str| {
        sources["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["path"].as_str().unwrap().starts_with(prefix))
            .map(|f| f["data_class"].as_str().unwrap().to_owned())
    };
    assert_eq!(class_of("alpha/").as_deref(), Some("public"));
    assert_eq!(class_of("beta/").as_deref(), Some("internal"));
    assert_eq!(class_of("secret/").as_deref(), Some("private"));
}

#[tokio::test]
async fn without_a_class_file_every_file_is_unclassified_and_still_shown() {
    let hub = serve("noclasses", None);
    let token = token(&hub).await;
    let (status, sources) = get(&hub, &token, "/api/rules/sources").await;
    assert_eq!(status, 200, "{sources}");
    let files = sources["files"].as_array().unwrap();
    assert_eq!(files.len(), 3);
    assert!(files.iter().all(|f| f["data_class"] == "unclassified"));
}

#[tokio::test]
async fn a_second_serve_in_the_same_config_folder_does_not_start() {
    let hub = serve("twice", Some(CLASSES));
    let out = run(&hub.dir, &["serve", "--port", &free_port().to_string()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("already running"),
        "{}",
        text(&out.stderr)
    );
}
