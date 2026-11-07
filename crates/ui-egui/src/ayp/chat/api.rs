//! Claude Messages API over the host's HTTPS hook (no SDK: Rust has none). Pure functions build the
//! request and read the answer; `spawn` runs one request on a worker thread with retries.
//! The key lives only in the `x-api-key` header: no error, log or Debug output carries it.
use crate::{HttpPost, HttpRequest, HttpResponse};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
pub const URL: &str = "https://api.anthropic.com/v1/messages";
const KEY_HINT: &str = "Nhập khoá Anthropic ở ⚙ Cài đặt AI";
const MAX_RETRIES: u32 = 3;
const MAX_WAIT_SECS: f64 = 30.0;

/// The Messages request body. History is append-only: `messages` is sent exactly as the caller kept it.
pub fn request_body(model: &str, system: &str, tools: &Value, messages: &[Value]) -> Value {
    json!({
        "model": model,
        "max_tokens": 16000,
        "fallbacks": "default",
        "output_config": {"effort": "medium"},
        "cache_control": {"type": "ephemeral"},
        "system": [{"type": "text", "text": system}],
        "tools": tools,
        "messages": messages,
    })
}

pub fn headers(key: &str) -> Vec<(String, String)> {
    [("x-api-key", key), ("anthropic-version", "2026-10-21"), ("content-type", "application/json"), ("anthropic-beta", "server-side-fallback-2026-07-01")]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[derive(Debug)]
pub enum ApiError {
    NoKey,
    Auth,
    Http { status: u16, message: String },
    Network(String),
    Bad(String),
}

impl ApiError {
    /// The Vietnamese text for the error card.
    pub fn vi(&self) -> String {
        match self {
            Self::NoKey => KEY_HINT.to_string(),
            Self::Auth => format!("Claude không nhận khoá này. {KEY_HINT}"),
            Self::Http { status, message } => format!("Claude báo lỗi {status}: {message}"),
            Self::Network(why) => format!("Không kết nối được tới Claude: {why}"),
            Self::Bad(why) => format!("Câu trả lời của Claude không đọc được: {why}"),
        }
    }
}

/// 2xx gives the message JSON (needs `content` array and `stop_reason`); 401/403 is `Auth`; anything
/// else is `Http` with the API's own `error.message` when there is one.
pub fn parse(resp: &HttpResponse) -> Result<Value, ApiError> {
    let json: Option<Value> = serde_json::from_str(&resp.body).ok();
    if (200..300).contains(&resp.status) {
        let v = json.ok_or_else(|| ApiError::Bad("không phải JSON".into()))?;
        let ok = v.get("content").is_some_and(Value::is_array) && v.get("stop_reason").is_some();
        return if ok { Ok(v) } else { Err(ApiError::Bad("thiếu content hoặc stop_reason".into())) };
    }
    if matches!(resp.status, 401 | 403) {
        return Err(ApiError::Auth);
    }
    let message = json
        .as_ref()
        .and_then(|v| v.pointer("/error/message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| resp.body.chars().take(200).collect());
    Err(ApiError::Http { status: resp.status, message })
}

/// How long to wait before the next try, or `None` to give up. `retry_after` is hostile: capped at 30 s.
pub fn retry_delay(status: u16, retry_after: Option<f64>, attempt: u32) -> Option<Duration> {
    if !matches!(status, 408 | 429 | 500 | 502 | 503 | 504 | 529) || attempt >= MAX_RETRIES {
        return None;
    }
    let secs = match retry_after {
        Some(s) if s.is_finite() && s >= 0.0 => s.min(MAX_WAIT_SECS),
        _ => f64::from(1u32 << attempt),
    };
    Some(Duration::from_secs_f64(secs))
}

/// Send `req` on the `ayp-chat` thread. The receiver gets one result; if `cancel` is set the worker
/// stops (mid-wait too) and sends nothing.
pub fn spawn(post: HttpPost, req: HttpRequest, cancel: Arc<AtomicBool>) -> Receiver<Result<Value, ApiError>> {
    let (tx, rx) = channel();
    let work = move || {
        let run = std::panic::AssertUnwindSafe(|| attempt_loop(&post, &req, &cancel));
        let out = std::panic::catch_unwind(run).unwrap_or_else(|_| Some(Err(ApiError::Bad("lỗi nội bộ khi gọi Claude".into()))));
        if let Some(out) = out {
            let _ = tx.send(out); // receiver gone: nothing to tell
        }
    };
    // If the thread cannot start, `tx` drops with the closure and the receiver sees a disconnect.
    let _ = std::thread::Builder::new().name("ayp-chat".into()).spawn(work);
    rx
}

/// `None` = cancelled.
fn attempt_loop(post: &HttpPost, req: &HttpRequest, cancel: &AtomicBool) -> Option<Result<Value, ApiError>> {
    for attempt in 0.. {
        if cancel.load(Ordering::SeqCst) {
            return None;
        }
        let resp = match post(req) {
            Ok(r) => r,
            Err(e) => return Some(Err(ApiError::Network(e))),
        };
        let wait = match retry_delay(resp.status, resp.retry_after, attempt) {
            Some(w) => w,
            None => return Some(parse(&resp)),
        };
        let mut left = wait;
        while !left.is_zero() {
            if cancel.load(Ordering::SeqCst) {
                return None;
            }
            let slice = left.min(Duration::from_millis(100));
            std::thread::sleep(slice);
            left -= slice;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    fn resp(status: u16, retry_after: Option<f64>, body: &str) -> HttpResponse {
        HttpResponse { status, retry_after, body: body.to_string() }
    }

    #[test]
    fn request_body_matches_the_wire_contract() {
        let tools = json!([{"name": "t"}]);
        let msgs = vec![json!({"role": "user", "content": "hi"})];
        let b = request_body("claude-sonnet-5-5", "sys", &tools, &msgs);
        assert_eq!(b["model"], "claude-sonnet-5-5");
        assert_eq!(b["max_tokens"], 16000);
        assert_eq!(b["fallbacks"], "default");
        assert_eq!(b["output_config"], json!({"effort": "medium"}));
        assert_eq!(b["cache_control"], json!({"type": "ephemeral"}));
        assert_eq!(b["system"], json!([{"type": "text", "text": "sys"}]));
        assert_eq!(b["tools"], tools);
        assert_eq!(b["messages"], json!(msgs));
        for absent in ["thinking", "temperature", "tool_choice"] {
            assert!(b.get(absent).is_none(), "{absent} must be absent");
        }
        let h = headers("k");
        let get = |n: &str| h.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str());
        assert_eq!(get("x-api-key"), Some("k"));
        assert_eq!(get("anthropic-version"), Some("2026-10-21"));
        assert_eq!(get("content-type"), Some("application/json"));
        assert_eq!(get("anthropic-beta"), Some("server-side-fallback-2026-07-01"));
    }

    #[test]
    fn parse_maps_statuses() {
        let ok = parse(&resp(200, None, r#"{"content":[],"stop_reason":"end_turn"}"#)).unwrap();
        assert_eq!(ok["stop_reason"], "end_turn");
        assert!(matches!(parse(&resp(200, None, r#"{"stop_reason":"x"}"#)), Err(ApiError::Bad(_))));
        assert!(matches!(parse(&resp(200, None, "nope")), Err(ApiError::Bad(_))));
        assert!(matches!(parse(&resp(401, None, "")), Err(ApiError::Auth)));
        assert!(matches!(parse(&resp(403, None, "{}")), Err(ApiError::Auth)));
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"too long"}}"#;
        match parse(&resp(400, None, body)) {
            Err(ApiError::Http { status: 400, message }) => assert_eq!(message, "too long"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(parse(&resp(500, None, "<html>")), Err(ApiError::Http { status: 500, .. })));
        assert!(ApiError::NoKey.vi().contains("Nhập khoá Anthropic ở ⚙ Cài đặt AI"));
        assert!(ApiError::Auth.vi().contains("Nhập khoá Anthropic ở ⚙ Cài đặt AI"));
        assert!(ApiError::Network("dns".into()).vi().contains("Không kết nối được tới Claude"));
        let v = ApiError::Http { status: 400, message: "too long".into() }.vi();
        assert!(v.contains("400") && v.contains("too long"));
    }

    #[test]
    fn retry_policy() {
        use std::time::Duration;
        assert_eq!(retry_delay(429, Some(2.0), 0), Some(Duration::from_secs(2)));
        assert_eq!(retry_delay(529, None, 0), Some(Duration::from_secs(1)));
        assert_eq!(retry_delay(529, None, 2), Some(Duration::from_secs(4)));
        assert_eq!(retry_delay(529, None, 3), None);
        assert_eq!(retry_delay(400, None, 0), None);
        assert_eq!(retry_delay(429, Some(999.0), 0), Some(Duration::from_secs(30)));
        assert_eq!(retry_delay(429, Some(f64::NAN), 1), Some(Duration::from_secs(2)));
        assert_eq!(retry_delay(429, Some(-5.0), 0), Some(Duration::from_secs(1)));
    }

    fn req() -> HttpRequest {
        HttpRequest { url: "u".into(), headers: vec![], body: "{}".into() }
    }

    #[test]
    fn spawn_retries_then_succeeds() {
        let calls = Arc::new(Mutex::new(0u32));
        let c = calls.clone();
        let post: HttpPost = Arc::new(move |_| {
            let mut n = c.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            *n += 1;
            Ok(if *n < 3 { resp(529, Some(0.0), "{}") } else { resp(200, None, r#"{"content":[],"stop_reason":"end_turn"}"#) })
        });
        let rx = spawn(post, req(), Arc::new(AtomicBool::new(false)));
        let got = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap().unwrap();
        assert_eq!(got["stop_reason"], "end_turn");
        assert_eq!(*calls.lock().unwrap(), 3);
    }

    #[test]
    fn spawn_turns_a_panic_and_a_network_error_into_errors() {
        let boom: HttpPost = Arc::new(|_| panic!("boom"));
        let rx = spawn(boom, req(), Arc::new(AtomicBool::new(false)));
        assert!(matches!(rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), Err(ApiError::Bad(_))));
        let down: HttpPost = Arc::new(|_| Err("offline".into()));
        let rx = spawn(down, req(), Arc::new(AtomicBool::new(false)));
        assert!(matches!(rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), Err(ApiError::Network(m)) if m == "offline"));
    }

    #[test]
    fn spawn_stops_when_cancelled() {
        let post: HttpPost = Arc::new(|_| Ok(resp(529, Some(30.0), "{}")));
        let cancel = Arc::new(AtomicBool::new(false));
        let rx = spawn(post, req(), cancel.clone());
        cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        // the worker exits without sending, so the channel disconnects well before the 30 s wait
        assert!(matches!(rx.recv_timeout(std::time::Duration::from_secs(5)), Err(std::sync::mpsc::RecvTimeoutError::Disconnected)));
    }
}
