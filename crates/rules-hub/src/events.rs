//! The event stream of one session, and the count of the streams each session holds.
//!
//! A copy of the route of tasks-hub (`server/events.rs`) without its usage log: the kit has the
//! store, its changes and the limits, but no route. Moving the route and the counter into the kit
//! is a later step.

use std::collections::HashMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use appkit_hub::{bearer, error_body, no_query};
use axum::body::{Body, Bytes};
use axum::extract::{RawQuery, State};
use axum::http::header::{self, HeaderMap, HeaderValue};
use axum::http::StatusCode;
use axum::response::Response;
use futures_core::Stream;
use serde_json::json;
use tokio::sync::{broadcast, mpsc};

use crate::answer::{engine_failure, engine_unavailable};
use crate::routes::Shared;
use crate::store::SHAPE;

/// The event streams that are open, by session token.
#[derive(Default)]
pub(crate) struct StreamCounter {
    by_token: Mutex<HashMap<String, usize>>,
}

/// One open stream; the place is given back when it is dropped.
pub(crate) struct StreamGuard {
    counter: Arc<StreamCounter>,
    token: String,
}

impl StreamCounter {
    /// A place for one more stream of this session, if it has fewer than `max` open.
    pub(crate) fn acquire(self: &Arc<Self>, token: &str, max: usize) -> Option<StreamGuard> {
        let mut map = self
            .by_token
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let open = map.entry(token.to_owned()).or_insert(0);
        if *open >= max {
            return None;
        }
        *open += 1;
        Some(StreamGuard {
            counter: Arc::clone(self),
            token: token.to_owned(),
        })
    }
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        let mut map = self
            .counter
            .by_token
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(open) = map.get_mut(&self.token) {
            *open = open.saturating_sub(1);
            if *open == 0 {
                map.remove(&self.token);
            }
        }
    }
}

/// The body of an event stream: what the task of the connection sends.
struct EventStream {
    rx: mpsc::Receiver<Bytes>,
}

impl Stream for EventStream {
    type Item = Result<Bytes, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx).map(|item| item.map(Ok))
    }
}

/// `GET /api/rules/events`: server-sent events. A `hello` first (the `rev` and `seq` the stream
/// starts at, and the `instance` of the hub), then a `rules` event for each change of the held
/// snapshot, `resync` when this stream lost events, and `: ping` while nothing happens. An event
/// is a hint with ids only; the page reads the snapshot again. There is no replay: a page that
/// connects reads the snapshot first.
///
/// The session is checked before every message, not only when the stream opens: a session that
/// was closed since (the oldest is closed when newer ones open) takes its streams with it, so no
/// id goes to a token that is no longer one.
pub(crate) async fn events(
    State(st): State<Arc<Shared>>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = no_query(query.as_deref()) {
        return response;
    }
    let rt = st.rt();
    if !rt.store.has_engine() {
        return engine_unavailable(&rt);
    }
    // The guard has checked the session; the token is the key of the count.
    let token = bearer(&headers).unwrap_or_default();
    let Some(guard) = rt
        .streams
        .acquire(&token, st.limits.max_streams_per_session)
    else {
        return error_body(
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_streams",
            "this session has too many open event streams",
        );
    };
    // Start from a known state: the snapshot is held before the stream is.
    if let Err(error) = rt.store.current().await {
        return engine_failure(error);
    }
    let (mut changes, seq, rev) = rt.store.subscribe();
    let (tx, rx) = mpsc::channel::<Bytes>(16);
    let heartbeat = st.limits.heartbeat;
    let instance = st.instance.clone();
    let shared = Arc::clone(&st);
    tokio::spawn(async move {
        let _guard = guard;
        let hello = format!(
            "event: hello\ndata: {}\n\n",
            json!({ "rev": rev, "seq": seq, "instance": instance })
        );
        // The hello too: the session may have been closed while the snapshot was read.
        if !shared.sessions.check(&token) {
            return;
        }
        if tx.send(Bytes::from(hello)).await.is_err() {
            return;
        }
        let mut ticker =
            tokio::time::interval_at(tokio::time::Instant::now() + heartbeat, heartbeat);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let message = tokio::select! {
                change = changes.recv() => match change {
                    Ok(c) => c.to_sse(SHAPE.event),
                    // This client was too slow for the buffer: it must read again.
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        "event: resync\ndata: {}\n\n".to_owned()
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                _ = ticker.tick() => ": ping\n\n".to_owned(),
                () = tx.closed() => return,
            };
            if !shared.sessions.check(&token) {
                return;
            }
            if tx.send(Bytes::from(message)).await.is_err() {
                return;
            }
        }
    });
    let mut response = Response::new(Body::from_stream(EventStream { rx }));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_holds_its_streams_up_to_the_limit_and_gets_each_place_back() {
        let counter = Arc::new(StreamCounter::default());
        let a = counter.acquire("t", 2).unwrap();
        let b = counter.acquire("t", 2).unwrap();
        assert!(counter.acquire("t", 2).is_none());
        // Another session is not counted against this one.
        assert!(counter.acquire("u", 2).is_some());
        drop(a);
        let c = counter.acquire("t", 2).unwrap();
        drop((b, c));
        assert!(counter.by_token.lock().unwrap().is_empty());
    }
}
