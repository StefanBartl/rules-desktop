//! The write side of the app: `POST /api/rules/ops` and `GET /api/rules/next_id`.
//!
//! The hub formats and parses no rule (WEB-13). It checks the form and the size of a request
//! before an engine process is asked, keeps the operations to a list, counts a write against the
//! limit of writes, passes the request on to the one door of the engine (`rules-ops/1`) and, when
//! the answer says something changed, reads the snapshot again before it answers, so that the next
//! read of the page is the new state and the open streams hear of it.

use std::sync::Arc;

use appkit_hub::{error_body, json_response, too_many};
use axum::body::Bytes;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde_json::{json, Value};

use crate::answer::{ask, engine_unavailable};
use crate::engine::Calls;
use crate::routes::Shared;

/// The fields of a request of `ops` (the engine checks the rest, strictly).
pub const OPS_FIELDS: [&str; 4] = ["schema", "client_op_id", "dry_run", "ops"];

/// The most operations in one request (the engine's limit too).
pub const OPS_MAX: usize = 100;

/// The operations the page may ask for. `waive`, `renew` and `remove` are the engine's too, but
/// they write the waiver file of a project and not a rule set: that is a later phase, and the hub
/// does not pass them on.
pub const OPS_ALLOWED: [&str; 5] = ["new", "set", "retire", "reinstate", "move"];

/// The form and the size of a request of `ops`, before an engine process is asked: what is plainly
/// not one is refused here, the rules of the rules are the engine's. Returns the request to send on.
///
/// # Errors
///
/// A short reason, no part of the request.
pub fn check_ops_request(body: &[u8]) -> Result<Value, String> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| "the body is not JSON".to_owned())?;
    let Some(map) = value.as_object() else {
        return Err("the request must be a JSON object".to_owned());
    };
    if let Some(key) = map.keys().find(|k| !OPS_FIELDS.contains(&k.as_str())) {
        return Err(format!("unknown field {key:.40} in the request"));
    }
    let Some(ops) = map.get("ops").and_then(Value::as_array) else {
        return Err("ops must be a list".to_owned());
    };
    if ops.is_empty() || ops.len() > OPS_MAX {
        return Err(format!("ops holds 1 to {OPS_MAX} operations"));
    }
    for (i, op) in ops.iter().enumerate() {
        let name = op
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("ops[{}] has no op", i + 1))?;
        if !OPS_ALLOWED.contains(&name) {
            return Err(format!("ops[{}]: this operation is not served", i + 1));
        }
    }
    if let Some(id) = map.get("client_op_id") {
        let ok = id.as_str().is_some_and(|s| {
            (1..=64).contains(&s.len())
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
        });
        if !ok {
            return Err("client_op_id is 1 to 64 letters, digits and . _ : -".to_owned());
        }
    }
    if map.get("dry_run").is_some_and(|d| !d.is_boolean()) {
        return Err("dry_run must be true or false".to_owned());
    }
    Ok(value)
}

/// Did the answer of `ops` say that something changed? A document this hub cannot read for it counts
/// as a change: it scans anyway.
fn changed(document: &str) -> bool {
    serde_json::from_str::<Value>(document)
        .ok()
        .and_then(|d| {
            d.get("results").and_then(Value::as_array).map(|entries| {
                entries.iter().any(|e| {
                    e.get("changed_ids")
                        .and_then(Value::as_array)
                        .is_some_and(|ids| !ids.is_empty())
                })
            })
        })
        .unwrap_or(true)
}

/// `POST /api/rules/ops`: changes, through the one door of the engine. The answer is the engine's
/// `rules.result`, with a part that failed told per operation (not all-or-nothing). A write that
/// changed something is followed by a scan before the answer.
pub(crate) async fn ops(State(st): State<Arc<Shared>>, body: Bytes) -> Response {
    let request = match check_ops_request(&body) {
        Ok(r) => r,
        Err(why) => {
            // A request that is not one still counts as a mutation.
            if !st.post_limit.allow() {
                return too_many();
            }
            return error_body(StatusCode::BAD_REQUEST, "bad_request", &why);
        }
    };
    let dry_run = request.get("dry_run").and_then(Value::as_bool) == Some(true);
    // A dry run changes nothing and was counted by the guard only: it costs the writes nothing.
    if !dry_run && !st.post_limit.allow() {
        return too_many();
    }
    let rt = st.rt();
    if !rt.store.has_engine() {
        return engine_unavailable(&rt);
    }
    let reply = match ask(&rt, Calls::Ops, Some(&request)).await {
        Ok(reply) => reply,
        Err(response) => {
            // A write that ended without an answer may have been carried out: look again, so the
            // page does not stand on an old state.
            if !dry_run {
                rt.store.request_rescan();
            }
            return response;
        }
    };
    if !dry_run && changed(&reply.text) {
        let _ = rt.store.refresh("ops").await;
    }
    json_response(StatusCode::OK, reply.text, None)
}

/// The longest family the page asks about: the engine takes 1 to 12 letters for a new one, and a
/// family it has already may be spelled otherwise.
const FAMILY_MAX: usize = 32;

/// The family of `next_id?family=<family>`, if there is one: 1 to 32 letters, digits, `_` or `-`.
/// No query at all asks for every family. What a family may be is the engine's to say.
pub(crate) fn family(query: Option<&str>) -> Result<Option<String>, &'static str> {
    let Some(query) = query.filter(|q| !q.is_empty()) else {
        return Ok(None);
    };
    let (key, value) = query.split_once('=').ok_or("a query part has no value")?;
    if key != "family" || value.contains('&') {
        return Err("unknown query parameter");
    }
    let ok = !value.is_empty()
        && value.len() <= FAMILY_MAX
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'));
    if ok {
        Ok(Some(value.to_owned()))
    } else {
        Err("family is 1 to 32 letters, digits, _ or -")
    }
}

/// `GET /api/rules/next_id` or `?family=<family>`: the next free id of a family, or of every
/// family, as the engine says it. It reads; the id is not reserved.
pub(crate) async fn next_id(State(st): State<Arc<Shared>>, RawQuery(query): RawQuery) -> Response {
    let params = match family(query.as_deref()) {
        Ok(f) => f.map(|f| json!({ "family": f })),
        Err(why) => return error_body(StatusCode::BAD_REQUEST, "bad_query", why),
    };
    let rt = st.rt();
    if !rt.store.has_engine() {
        return engine_unavailable(&rt);
    }
    match ask(&rt, Calls::NextId, params.as_ref()).await {
        Ok(reply) => json_response(StatusCode::OK, reply.text, None),
        Err(response) => response,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(body: &str) -> String {
        check_ops_request(body.as_bytes()).expect_err("refused")
    }

    #[test]
    fn a_request_of_the_served_operations_passes() {
        for op in OPS_ALLOWED {
            let body = format!(
                r#"{{"schema":1,"client_op_id":"c-1","dry_run":true,"ops":[{{"op":"{op}"}}]}}"#
            );
            assert!(check_ops_request(body.as_bytes()).is_ok(), "{op}");
        }
    }

    #[test]
    fn what_is_plainly_not_a_request_is_refused_before_the_engine_is_asked() {
        assert_eq!(refused("nope"), "the body is not JSON");
        assert_eq!(refused("[]"), "the request must be a JSON object");
        assert!(refused(r#"{"ops":[{"op":"set"}],"x":1}"#).starts_with("unknown field x"));
        assert_eq!(refused(r#"{"ops":"set"}"#), "ops must be a list");
        assert_eq!(refused(r#"{"ops":[]}"#), "ops holds 1 to 100 operations");
        let many = format!(r#"{{"ops":[{}]}}"#, vec![r#"{"op":"set"}"#; 101].join(","));
        assert_eq!(refused(&many), "ops holds 1 to 100 operations");
        assert_eq!(refused(r#"{"ops":[{}]}"#), "ops[1] has no op");
        assert_eq!(refused(r#"{"ops":[{"op":7}]}"#), "ops[1] has no op");
        assert_eq!(
            refused(r#"{"ops":[{"op":"set"},{"op":"explode"}]}"#),
            "ops[2]: this operation is not served"
        );
        assert_eq!(
            refused(r#"{"ops":[{"op":"set"}],"dry_run":"yes"}"#),
            "dry_run must be true or false"
        );
        for id in ["", "a b", "x/y", &"a".repeat(65)] {
            let body = json!({"client_op_id": id, "ops": [{"op": "set"}]}).to_string();
            assert!(refused(&body).starts_with("client_op_id"), "{id:?}");
        }
    }

    #[test]
    fn the_waiver_operations_are_not_served() {
        for op in ["waive", "renew", "remove"] {
            let body = json!({"ops": [{"op": op}]}).to_string();
            assert_eq!(
                refused(&body),
                "ops[1]: this operation is not served",
                "{op}"
            );
        }
    }

    #[test]
    fn an_answer_with_a_changed_id_is_a_change_and_one_without_is_not() {
        assert!(changed(
            r#"{"results":[{"changed_ids":[]},{"changed_ids":["DEP-01"]}]}"#
        ));
        assert!(!changed(r#"{"results":[{"changed_ids":[]}]}"#));
        assert!(!changed(r#"{"results":[]}"#));
        // A document the hub cannot read for it: look again.
        assert!(changed("not json"));
        assert!(changed(r#"{"kind":"rules.result"}"#));
    }

    #[test]
    fn the_family_is_short_and_plain_or_there_is_none() {
        assert_eq!(family(None), Ok(None));
        assert_eq!(family(Some("")), Ok(None));
        assert_eq!(family(Some("family=DEP")), Ok(Some("DEP".to_owned())));
        assert_eq!(
            family(Some("family=Dep-01_x")),
            Ok(Some("Dep-01_x".to_owned()))
        );
        for bad in [
            "family=",
            "family=a%20b",
            "family=a&family=b",
            "x=1",
            "family",
            &format!("family={}", "A".repeat(33)),
        ] {
            assert!(family(Some(bad)).is_err(), "{bad}");
        }
    }
}
