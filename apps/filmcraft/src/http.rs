//! AYP: the HTTPS POST the AI chat calls through `HostHooks::http_post` (ureq 3 with rustls +
//! RustCrypto and the operating system's certificate verifier, as `crates/speech` does).

use filmcraft_ui_egui::{HttpPost, HttpRequest, HttpResponse};
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

/// Longest answer we read; anything bigger is an error, not an allocation.
const MAX_BODY: u64 = 8 * 1024 * 1024;

/// Longest `retry-after` we pass on (seconds); a hostile `1e308` must not reach `Duration::from_secs_f64`.
/// The HTTP-date form of the header is not parsed and comes back as `None`.
const MAX_RETRY_AFTER: f64 = 3600.0;

fn agent() -> ureq::Agent {
    let provider = Arc::new(rustls_rustcrypto::provider());
    ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::Rustls)
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .unversioned_rustls_crypto_provider(provider)
                .build(),
        )
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_global(Some(Duration::from_secs(600)))
        // A 429 or 500 is an answer the chat reacts to, not a transport error.
        .http_status_as_error(false)
        .build()
        .into()
}

fn post(agent: &ureq::Agent, req: &HttpRequest) -> Result<HttpResponse, String> {
    let mut builder = agent.post(&req.url);
    for (k, v) in &req.headers {
        builder = builder.header(k, v);
    }
    let resp = builder.send(req.body.as_str()).map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s >= 0.0)
        .map(|s| s.min(MAX_RETRY_AFTER));
    let mut body = String::new();
    // One byte past the cap tells "exactly at the cap" from "too long".
    let n = resp.into_body().into_reader().take(MAX_BODY + 1).read_to_string(&mut body).map_err(|e| e.to_string())?;
    if n as u64 > MAX_BODY {
        return Err("Phản hồi quá lớn (hơn 8 MB)".to_string());
    }
    Ok(HttpResponse { status, retry_after, body })
}

/// The hook for `HostHooks::http_post`; the agent is built once and shared.
pub fn http_post() -> HttpPost {
    let agent = agent();
    Arc::new(move |req: &HttpRequest| post(&agent, req))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    /// Serve one canned reply, return the URL.
    fn serve(reply: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}/v1/messages", listener.local_addr().expect("addr"));
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let _ = s.write_all(reply.as_bytes());
            }
        });
        url
    }

    fn req(url: String) -> HttpRequest {
        HttpRequest { url, headers: vec![("x-test".into(), "1".into())], body: "{}".into() }
    }

    #[test]
    fn a_429_is_an_answer_with_retry_after() {
        let url = serve("HTTP/1.1 429 Too Many Requests\r\nretry-after: 2\r\ncontent-length: 4\r\nconnection: close\r\n\r\nslow");
        let r = http_post()(&req(url)).expect("a 429 is Ok");
        assert_eq!(r, HttpResponse { status: 429, retry_after: Some(2.0), body: "slow".into() });
    }

    #[test]
    fn retry_after_is_clamped_and_junk_is_none() {
        let url = serve("HTTP/1.1 429 X\r\nretry-after: 1e308\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
        assert_eq!(http_post()(&req(url)).expect("Ok").retry_after, Some(3600.0));
        let url = serve("HTTP/1.1 429 X\r\nretry-after: abc\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
        assert_eq!(http_post()(&req(url)).expect("Ok").retry_after, None);
    }

    #[test]
    fn a_200_carries_its_body() {
        let url = serve("HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok");
        let r = http_post()(&req(url)).expect("200");
        assert_eq!((r.status, r.retry_after, r.body.as_str()), (200, None, "ok"));
    }

    #[test]
    fn a_refused_connection_is_an_error() {
        assert!(http_post()(&req("http://127.0.0.1:1/x".into())).is_err());
    }
}
