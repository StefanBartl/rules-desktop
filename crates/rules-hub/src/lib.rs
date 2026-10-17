//! The hub of rules-desktop, on `appkit-hub`. It has its names and the one route a session reads so
//! far (`about`); the engine, the views and the commands of `tasks-hub` arrive with the read-only UI
//! phase.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use appkit_core::authn::{Clock, Secret};
use appkit_core::identity::AppIdentity;
use appkit_hub::{
    json_response, serve, Core, GuardRules, HubStatus, Limits, Page, Pages, RunningServer, Runtime,
    ServeParts,
};
use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;

/// The names of the app: its config folder, its program, its variables, its port and its routes.
pub const IDENTITY: AppIdentity = AppIdentity {
    app: "rules-desktop",
    binary: "rules-hub",
    env_prefix: "RULES_DESKTOP",
    title: "Rules",
    default_port: 17616,
    route_ns: "rules",
};

/// The version of this build, as `rules-hub --version` and `about` say it.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// What the hub keeps while it runs: no engine yet, only the status `hello` reports.
pub struct RulesRuntime {
    status: HubStatus,
}

impl Runtime for RulesRuntime {
    type Target = ();

    fn status(&self) -> &HubStatus {
        &self.status
    }

    fn stop(&self) {}
}

/// No page yet: the placeholder of the core is served.
struct NoPages;

impl Pages for NoPages {
    fn page(&self, _request_path: &str) -> Option<Page> {
        None
    }

    fn has_index(&self) -> bool {
        false
    }
}

/// The routes that need a session, by name under `/api/rules/`.
pub const DATA_ROUTES: &[&str] = &["about"];

fn about_body() -> String {
    serde_json::json!({ "app": IDENTITY.app, "version": version() }).to_string()
}

fn routes(_core: &Arc<Core<RulesRuntime>>) -> Router<Arc<Core<RulesRuntime>>> {
    Router::new().route(
        &IDENTITY.route("about"),
        get(|| async { json_response(StatusCode::OK, about_body(), None) }),
    )
}

/// What a start of the hub is given; the rest are the hub's own names.
pub struct Start {
    /// Must be loopback; port 0 picks a free port.
    pub addr: SocketAddr,
    pub limits: Limits,
    pub clock: Arc<dyn Clock>,
    pub pairing_ttl: Duration,
    pub control_secret: Secret,
    pub instance: Secret,
    pub status: HubStatus,
}

/// Starts the hub.
///
/// # Errors
///
/// When the address is not loopback or cannot be bound.
pub async fn start(s: Start) -> std::io::Result<RunningServer> {
    serve(
        ServeParts {
            addr: s.addr,
            limits: s.limits,
            clock: s.clock,
            pairing_ttl: s.pairing_ttl,
            control_secret: s.control_secret,
            instance: s.instance,
            app: IDENTITY.app,
            version: version(),
            guard_rules: GuardRules::new(IDENTITY.route("hello"), []),
            ui: Arc::new(NoPages),
            runtime: RulesRuntime { status: s.status },
            build: Arc::new(|()| RulesRuntime {
                status: HubStatus {
                    engine_ready: false,
                    problem: None,
                    capabilities: Vec::new(),
                },
            }),
            vaults: None,
        },
        routes,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_semver_shaped() {
        assert_eq!(version().split('.').count(), 3);
    }

    #[test]
    fn about_names_the_app_and_its_version() {
        let v: serde_json::Value = serde_json::from_str(&about_body()).unwrap();
        assert_eq!(v["app"], "rules-desktop");
        assert_eq!(v["version"], version());
    }
}
