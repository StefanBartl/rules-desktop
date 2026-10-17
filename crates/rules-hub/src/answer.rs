//! How a question to the engine is answered: the refusals and the unavailable engine.

use appkit_hub::error_body;
use axum::http::header::{self, HeaderValue};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::engine::{EngineError, Method, Reply};
use crate::runtime::RulesRuntime;

/// What a request without an engine gets: the hub is up, the engine is not.
pub(crate) fn engine_unavailable(rt: &RulesRuntime) -> Response {
    let body = json!({ "error": {
        "code": "engine_unavailable",
        "message": "the engine is not ready",
        "problem": rt.status.problem,
    }});
    (StatusCode::SERVICE_UNAVAILABLE, axum::Json(body)).into_response()
}

/// Map what went wrong with a question to an answer. The engine's own refusals keep their code
/// and their message (the contract promises that they hold no path of this machine), cut to a
/// word and a line; everything else is told in one word, and the detail goes to the terminal of
/// whoever runs the hub.
pub(crate) fn engine_failure(error: EngineError) -> Response {
    match error {
        EngineError::Refused(r) => {
            let status = match r.code.as_str() {
                "invalid_argument" | "unsupported_schema" => StatusCode::BAD_REQUEST,
                "not_found" => StatusCode::NOT_FOUND,
                "payload_too_large" => StatusCode::PAYLOAD_TOO_LARGE,
                // The engine could not do its job: that is the server's side.
                _ => StatusCode::BAD_GATEWAY,
            };
            let code_ok = !r.code.is_empty()
                && r.code.len() <= 48
                && r.code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
            let code = if code_ok {
                r.code
            } else {
                "engine_failed".to_owned()
            };
            let message: String = r
                .message
                .chars()
                .filter(|c| !c.is_control())
                .take(512)
                .collect();
            let body = json!({ "error": {
                "code": code,
                "message": message,
                "retryable": r.retryable,
            }});
            (status, axum::Json(body)).into_response()
        }
        EngineError::BadRequest(message) => {
            error_body(StatusCode::BAD_REQUEST, "bad_request", &message)
        }
        EngineError::Busy => {
            let mut r = error_body(
                StatusCode::SERVICE_UNAVAILABLE,
                "busy",
                "the engine is busy, try again",
            );
            r.headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
            r
        }
        EngineError::Timeout => error_body(
            StatusCode::GATEWAY_TIMEOUT,
            "engine_timeout",
            "the engine did not answer in time",
        ),
        other => {
            eprintln!("engine: {other}");
            error_body(
                StatusCode::BAD_GATEWAY,
                "engine_failed",
                "the engine could not answer",
            )
        }
    }
}

/// Ask the engine of the runtime one question.
// The error is the answer to send: a response is large, and rare.
#[allow(clippy::result_large_err)]
pub(crate) async fn ask(
    rt: &RulesRuntime,
    method: Method,
    params: Option<&serde_json::Value>,
) -> Result<Reply, Response> {
    let Some(engine) = &rt.engine else {
        return Err(engine_unavailable(rt));
    };
    engine.call(method, params).await.map_err(engine_failure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineRefusal;

    async fn body(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn refusal(code: &str, message: &str) -> EngineError {
        EngineError::Refused(EngineRefusal {
            code: code.to_owned(),
            message: message.to_owned(),
            retryable: false,
            text: String::new(),
        })
    }

    #[tokio::test]
    async fn a_refusal_is_cut_to_a_word_and_a_line() {
        let r = engine_failure(EngineError::Refused(EngineRefusal {
            code: "../../etc".to_owned(),
            message: format!("line one\nline two\u{1b}[31m{}", "x".repeat(10_000)),
            retryable: true,
            text: String::new(),
        }));
        assert_eq!(r.status(), StatusCode::BAD_GATEWAY);
        let doc = body(r).await;
        assert_eq!(doc["error"]["code"], "engine_failed");
        let message = doc["error"]["message"].as_str().unwrap();
        assert!(message.chars().count() <= 512, "{}", message.len());
        assert!(!message.contains(|c: char| c.is_control()));
        assert_eq!(doc["error"]["retryable"], true);
    }

    #[tokio::test]
    async fn engine_failures_become_the_right_status_and_tell_no_detail() {
        let cases = [
            (refusal("invalid_argument", "m"), 400),
            (refusal("unsupported_schema", "m"), 400),
            (refusal("not_found", "no rule with the id X"), 404),
            (refusal("payload_too_large", "m"), 413),
            (refusal("io", "m"), 502),
            (refusal("internal", "m"), 502),
            (EngineError::Busy, 503),
            (EngineError::Timeout, 504),
            (EngineError::BadRequest("x".to_owned()), 400),
            (
                EngineError::Spawn("C:\\Users\\someone\\nvim".to_owned()),
                502,
            ),
            (
                EngineError::Failed {
                    code: Some(1),
                    detail: "C:\\Users\\someone\\secret".to_owned(),
                },
                502,
            ),
            (EngineError::Protocol("x".to_owned()), 502),
        ];
        for (error, status) in cases {
            let shown = error.to_string();
            let r = engine_failure(error);
            assert_eq!(r.status().as_u16(), status, "{shown}");
            assert!(!body(r).await.to_string().contains("someone"), "{shown}");
        }
        assert_eq!(
            engine_failure(EngineError::Busy)
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok()),
            Some("1")
        );
    }
}
