//! The hub of rules-desktop, on `appkit-hub`: its names, the engine seam to rules.nvim
//! (`rules-export/1`, read only), the data classes of this device (`sources.json`), the routes
//! a session reads (`snapshot`, `rule`, `sources`, `search`, `events`, `about`, and `rescan`) and
//! what keeps them fresh (the watcher of the vault, the refresher, the search index).
//! The page and the commands of `tasks-hub` arrive with the later steps of the read-only UI phase.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use appkit_core::authn::{Clock, Secret};
use appkit_core::identity::AppIdentity;
use appkit_hub::{serve, GuardRules, HubStatus, Limits, Page, Pages, RunningServer, ServeParts};

mod answer;
pub mod bodies;
pub mod engine;
mod events;
mod routes;
mod runtime;
pub mod sources;
pub mod store;
pub mod watch;

pub use routes::{MakeRoute, Shared, MAX_ID_BYTES, ROUTES};
pub use runtime::RulesRuntime;

use bodies::HostedEngine;
use runtime::Parts;
use sources::SourcePolicy;
use store::RefreshOptions;

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

/// The routes that need a session, by name under `/api/rules/`: every route of [`ROUTES`], so the
/// security matrix tries each one that is served.
pub static DATA_ROUTES: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| ROUTES.iter().map(|(name, _)| *name).collect());

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
    /// The engine the routes ask; `None`: the hub is up and every data route says the engine is
    /// not ready. The search reads the text of the rules only while its host runs
    /// ([`bodies::BodiesEngine`]).
    pub engine: Option<Arc<dyn HostedEngine>>,
    /// The folder the engine reads, watched for changes; `None`: only the timer of the refresher
    /// and the rescan the page asks for see a change.
    pub vault: Option<PathBuf>,
    /// The timings of the refresher.
    pub refresh: RefreshOptions,
    /// The data classes of this device. The caller loads them with [`SourcePolicy::load`] before
    /// it starts the hub and must not start it when that fails; the program has no such caller yet
    /// (`serve` comes later). The default classifies nothing.
    pub sources: Arc<SourcePolicy>,
}

/// Starts the hub.
///
/// # Errors
///
/// When the address is not loopback or cannot be bound.
pub async fn start(s: Start) -> std::io::Result<RunningServer> {
    let max_age = s.limits.snapshot_max_age;
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
            runtime: RulesRuntime::new(Parts {
                engine: s.engine,
                status: s.status,
                sources: s.sources,
                max_age,
                vault: s.vault,
                refresh: s.refresh,
            }),
            // There is one vault and no list to choose from, so this is never called.
            build: Arc::new(move |()| {
                RulesRuntime::new(Parts {
                    engine: None,
                    status: HubStatus {
                        engine_ready: false,
                        problem: None,
                        capabilities: Vec::new(),
                    },
                    sources: Arc::default(),
                    max_age,
                    vault: None,
                    refresh: RefreshOptions::default(),
                })
            }),
            vaults: None,
        },
        routes::app_routes,
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
}
