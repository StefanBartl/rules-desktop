//! The job that keeps the search index on the state of the held snapshot.
//!
//! The kit's `spawn_indexer` builds once and then lets `SearchIndex::apply` follow each change.
//! `apply` keeps a page of named ids with the `rev` the page says, and a page is read from the
//! state of the rule sets when it is read: a second change before it is read (two saves within
//! one debounce, a checkout) stamps the index with the next `rev` while only the ids of the first
//! were read again, and `apply` then takes the next change for one it has, so its ids keep their
//! old text. A page that fails leaves the index as it was and its ids are never read again. Both
//! are seen here from what [`BodiesEngine`] noted of the pages, and the index is built again; a
//! build that fails is tried again at the next change instead of an `apply`.
//!
//! Follow-up (`appkit/search-apply-baut-neu-bei-fremdem-rev-oder-fehler`): this belongs into the
//! kit's `apply`; the hub then goes back to `spawn_indexer`.

use std::sync::Arc;
use std::time::Duration;

use appkit_hub::search::SearchIndex;
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

use crate::bodies::BodiesEngine;
use crate::store::{Change, SnapshotStore};

/// How long after the start of the hub the index is first built (as the kit's indexer).
const FIRST_BUILD_DELAY: Duration = Duration::from_secs(2);

/// The index and what its pages came to.
pub(crate) struct Indexer {
    index: Arc<SearchIndex>,
    bodies: Arc<BodiesEngine>,
    /// The last build failed, or a change was not followed: the next change builds.
    stale: bool,
}

impl Indexer {
    /// `index` must read its pages through `bodies`.
    pub(crate) fn new(index: Arc<SearchIndex>, bodies: Arc<BodiesEngine>) -> Self {
        Self {
            index,
            bodies,
            stale: false,
        }
    }

    /// Read every text again.
    pub(crate) async fn build(&mut self) {
        self.bodies.take_log();
        let result = self.index.build().await;
        self.bodies.take_log();
        if let Err(e) = &result {
            eprintln!("search index: {e}");
        }
        self.stale = result.is_err();
    }

    /// Follow a change the store reported.
    pub(crate) async fn follow(&mut self, change: &Change) {
        if self.stale {
            self.build().await;
            return;
        }
        self.bodies.take_log();
        self.index.apply(change).await;
        let log = self.bodies.take_log();
        let other_state = log
            .id_revs
            .iter()
            .any(|rev| Some(rev) != change.rev.as_ref());
        // A change that only removes reads no page: `apply` keeps the rev of the state before, so the same state coming
        // back (a checkout and back) would be taken for one the index has, and its rules never read again.
        let removed_only = change.changed.is_empty()
            && change.added.is_empty()
            && !change.removed.is_empty()
            && !change.truncated;
        if log.failed || other_state || removed_only {
            self.build().await;
        }
    }
}

/// Run the index for the life of the hub: a first build after a moment, then what the store
/// reports.
pub(crate) fn spawn(mut indexer: Indexer, store: &Arc<SnapshotStore>) -> JoinHandle<()> {
    let mut changes = store.subscribe_quiet();
    tokio::spawn(async move {
        tokio::time::sleep(FIRST_BUILD_DELAY).await;
        indexer.build().await;
        loop {
            match changes.recv().await {
                Ok(change) => indexer.follow(&change).await,
                Err(RecvError::Lagged(_)) => indexer.build().await,
                Err(RecvError::Closed) => return,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use appkit_hub::search::Query;

    use super::*;
    use crate::bodies::tests::{hosted, Fake};
    use crate::engine::{Calls, Engine};

    fn indexer(fake: &Arc<Fake>) -> Indexer {
        let bodies = Arc::new(hosted(fake));
        let index = SearchIndex::new(Arc::clone(&bodies) as Arc<dyn Engine>, Calls::Bodies);
        Indexer::new(index, bodies)
    }

    fn change(rev: &str, changed: &[&str]) -> Change {
        Change {
            seq: 1,
            rev: Some(rev.to_owned()),
            cause: "fs",
            changed: changed.iter().map(|s| (*s).to_owned()).collect(),
            added: Vec::new(),
            removed: Vec::new(),
            truncated: false,
        }
    }

    fn finds(indexer: &Indexer, words: &str) -> Vec<String> {
        let outcome = indexer
            .index
            .search(&Query::parse(words).unwrap(), 10)
            .expect("an index");
        outcome.hits.into_iter().map(|h| h.id).collect()
    }

    /// Two changes before the first is read: its page is of the second state, which the index
    /// then takes for one it has. The second rule must not keep its old text.
    #[tokio::test]
    async fn a_page_of_a_later_state_than_its_change_builds_the_index_again() {
        let fake = Fake::new(&[("A-1", "alpha old"), ("A-2", "beta old")]);
        let mut indexer = indexer(&fake);
        indexer.build().await;
        assert_eq!(finds(&indexer, "beta old"), ["A-2"]);

        // rev:2 changed A-1, rev:3 changed A-2; the page for rev:2 is read at rev:3.
        *fake.rules.lock().unwrap() = vec![("A-1", "alpha new"), ("A-2", "beta new")];
        *fake.rev.lock().unwrap() = "rev:3".to_owned();
        indexer.follow(&change("rev:2", &["A-1"])).await;
        indexer.follow(&change("rev:3", &["A-2"])).await;
        assert_eq!(finds(&indexer, "alpha new"), ["A-1"]);
        assert_eq!(finds(&indexer, "beta new"), ["A-2"]);
        assert!(finds(&indexer, "old").is_empty());
    }

    /// A page for a change that fails leaves its ids with their old text: the index is built
    /// again, and when that fails too, at the next change.
    #[tokio::test]
    async fn a_change_that_could_not_be_followed_is_built_at_the_next_one() {
        let fake = Fake::new(&[("A-1", "alpha old"), ("A-2", "beta old")]);
        let mut indexer = indexer(&fake);
        indexer.build().await;

        *fake.rules.lock().unwrap() = vec![("A-1", "alpha new"), ("A-2", "beta old")];
        *fake.rev.lock().unwrap() = "rev:2".to_owned();
        fake.host.store(false, Ordering::SeqCst);
        indexer.follow(&change("rev:2", &["A-1"])).await;
        assert!(indexer.stale);

        fake.host.store(true, Ordering::SeqCst);
        *fake.rules.lock().unwrap() = vec![("A-1", "alpha new"), ("A-2", "beta new")];
        *fake.rev.lock().unwrap() = "rev:3".to_owned();
        indexer.follow(&change("rev:3", &["A-2"])).await;
        assert!(!indexer.stale);
        assert_eq!(finds(&indexer, "alpha new"), ["A-1"]);
        assert_eq!(finds(&indexer, "beta new"), ["A-2"]);
    }

    /// A rule goes and comes back with the state it had: the index must have it again.
    #[tokio::test]
    async fn a_rule_that_goes_and_comes_back_with_the_old_state_is_found_again() {
        let fake = Fake::new(&[("A-1", "alpha old"), ("A-2", "beta old")]);
        let mut indexer = indexer(&fake);
        indexer.build().await;
        *fake.rules.lock().unwrap() = vec![("A-1", "alpha old")];
        *fake.rev.lock().unwrap() = "rev:2".to_owned();
        let mut gone = change("rev:2", &[]);
        gone.removed = vec!["A-2".to_owned()];
        indexer.follow(&gone).await;
        assert!(finds(&indexer, "beta").is_empty());
        // Back to the first state (the rev of rules.nvim is made of the files): A-2 is added at rev:1 again.
        *fake.rules.lock().unwrap() = vec![("A-1", "alpha old"), ("A-2", "beta old")];
        *fake.rev.lock().unwrap() = "rev:1".to_owned();
        let mut back = change("rev:1", &[]);
        back.added = vec!["A-2".to_owned()];
        indexer.follow(&back).await;
        assert_eq!(finds(&indexer, "beta old"), ["A-2"]);
    }

    /// The common case reads only the named ids.
    #[tokio::test]
    async fn a_change_of_one_rule_reads_that_rule_only() {
        let fake = Fake::new(&[("A-1", "alpha old"), ("A-2", "beta old")]);
        let mut indexer = indexer(&fake);
        indexer.build().await;
        let before = fake.asked().len();
        *fake.rules.lock().unwrap() = vec![("A-1", "alpha new"), ("A-2", "beta old")];
        *fake.rev.lock().unwrap() = "rev:2".to_owned();
        indexer.follow(&change("rev:2", &["A-1"])).await;
        let asked: Vec<String> = fake.asked()[before..].to_vec();
        assert_eq!(
            asked.iter().filter(|a| *a == "rule").count(),
            1,
            "{asked:?}"
        );
        assert_eq!(finds(&indexer, "alpha new"), ["A-1"]);
    }
}
