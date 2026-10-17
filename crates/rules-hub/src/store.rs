//! The snapshot the hub holds, and the events that tell it changed: the store is the kit's, this
//! module states what the rules hub calls its documents.

pub use appkit_hub::store::{
    spawn_refresher, Cached, Change, RefreshOptions, SnapshotShape, SnapshotStore, StoreOptions,
};

use crate::engine::Calls;

/// The snapshot of the rules engine: the list is `rules`, each with its `id` and the `etag` of its
/// file, and the event is `rules`. A rule has no `problems`; what went wrong while loading is
/// `load_errors` of the whole document, and the engine does not trust a file younger than two
/// seconds anyway, so no state is waited out.
pub const SHAPE: SnapshotShape = SnapshotShape {
    call: Calls::Snapshot,
    items_key: "rules",
    event: "rules",
    transitional: &[],
};

#[cfg(test)]
mod tests {
    use super::*;

    /// A client listens for `event: rules`, and the engine writes its list under `rules`: both are
    /// part of the contract, a rename is a migration.
    #[test]
    fn the_names_of_the_rules_contract_stay() {
        assert_eq!(SHAPE.items_key, "rules");
        assert_eq!(SHAPE.event, "rules");
        assert_eq!(SHAPE.call, Calls::Snapshot);
        assert!(SHAPE.transitional.is_empty());
        let c = Change {
            seq: 1,
            rev: None,
            cause: "fs",
            changed: Vec::new(),
            added: Vec::new(),
            removed: Vec::new(),
            truncated: false,
        };
        assert!(c.to_sse(SHAPE.event).starts_with("event: rules\ndata: {"));
    }
}
