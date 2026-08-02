//! Blocking Web API transport with envelope handling, 429 retries, and
//! cursor pagination.

use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::config::Workspace;
use crate::error::{Error, Result};

const BASE: &str = "https://slack.com/api";
/// Total elapsed time we are willing to spend waiting out 429s per call.
const RETRY_BUDGET: Duration = Duration::from_secs(60);

pub struct Client {
    http: reqwest::blocking::Client,
    token: String,
    cookie: Option<String>,
    verbose: bool,
}

impl Client {
    /// Build a client for a workspace. Session (`xoxc-`) tokens require the
    /// `d` cookie and fail here without one.
    pub fn new(workspace: &Workspace, verbose: bool) -> Result<Client> {
        let cookie = workspace
            .cookie
            .as_ref()
            .map(|c| c.strip_prefix("d=").unwrap_or(c).to_string());
        if workspace.token.starts_with("xoxc-") && cookie.is_none() {
            return Err(Error::Auth(
                "session (xoxc) tokens require the `d` cookie — re-run `slack auth add`".into(),
            ));
        }
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("slack-cli/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Client {
            http,
            token: workspace.token.clone(),
            cookie,
            verbose,
        })
    }

    fn request(&self, method: &str) -> reqwest::blocking::RequestBuilder {
        let mut req = self
            .http
            .post(format!("{BASE}/{method}"))
            .bearer_auth(&self.token);
        if let Some(c) = &self.cookie {
            req = req.header(reqwest::header::COOKIE, format!("d={c}"));
        }
        req
    }

    /// Call a Web API method with form parameters.
    ///
    /// Retries 429s within a 60-second elapsed budget (a 429 means the request
    /// was rejected, so retrying never replays a mutation). Transport failures
    /// are never retried.
    pub fn call(&self, method: &str, params: &[(&str, String)]) -> Result<Value> {
        self.send(method, |req| req.form(&params.to_vec()))
    }

    /// Call a Web API method with a JSON body (raw `api --data` path).
    pub fn call_json(&self, method: &str, body: &Value) -> Result<Value> {
        self.send(method, |req| req.json(body))
    }

    fn send(
        &self,
        method: &str,
        build: impl Fn(reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder,
    ) -> Result<Value> {
        let start = Instant::now();
        loop {
            if self.verbose {
                eprintln!("-> {method}");
            }
            let resp = build(self.request(method)).send()?;
            if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                let wait = resp
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(2);
                if start.elapsed() + Duration::from_secs(wait) > RETRY_BUDGET {
                    return Err(Error::RateLimit {
                        method: method.to_string(),
                    });
                }
                eprintln!("rate limited on {method}; waiting {wait}s");
                thread::sleep(Duration::from_secs(wait));
                continue;
            }
            let v: Value = resp.json()?;
            if v.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                return Ok(v);
            }
            let code = v
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown_error")
                .to_string();
            return Err(match code.as_str() {
                "invalid_auth" | "not_authed" | "token_revoked" | "token_expired"
                | "account_inactive" => Error::Auth(format!(
                    "{method}: {code} — the stored token/cookie is invalid or expired"
                )),
                _ => Error::Api {
                    method: method.to_string(),
                    code,
                },
            });
        }
    }

    /// POST raw bytes to a pre-signed upload URL.
    ///
    /// Deliberately unauthenticated: the URL carries its own signature, and
    /// the host is not the Web API, so the response is plain text rather than
    /// a Slack envelope. Never retried — a repeat would duplicate the upload.
    pub fn put_bytes(&self, url: &str, bytes: Vec<u8>) -> Result<()> {
        if self.verbose {
            eprintln!("-> upload {url}");
        }
        let resp = self.http.post(url).body(bytes).send()?;
        if !resp.status().is_success() {
            return Err(Error::Api {
                method: "file upload".into(),
                code: format!("upload URL returned HTTP {}", resp.status()),
            });
        }
        Ok(())
    }

    /// Follow cursor pagination on a list method, collecting `key` items.
    ///
    /// `limit: Some(n)` stops after n items; `None` (`--all`) follows every
    /// cursor. Returns the items plus the unconsumed `next_cursor`, if any.
    pub fn paged(
        &self,
        method: &str,
        params: &[(&str, String)],
        key: &str,
        limit: Option<usize>,
    ) -> Result<(Vec<Value>, Option<String>)> {
        let mut out: Vec<Value> = Vec::new();
        let mut cursor = String::new();
        loop {
            let mut p = params.to_vec();
            if !cursor.is_empty() {
                p.push(("cursor", cursor.clone()));
            }
            let v = self.call(method, &p)?;
            if let Some(items) = v.get(key).and_then(Value::as_array) {
                out.extend(items.iter().cloned());
            }
            cursor = v
                .pointer("/response_metadata/next_cursor")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let done = cursor.is_empty() || limit.is_some_and(|n| out.len() >= n);
            if done {
                break;
            }
        }
        if let Some(n) = limit
            && out.len() > n
        {
            out.truncate(n);
            // Cursor no longer aligns with the truncation point; report it
            // anyway so --json callers can continue from the last full page.
        }
        let next = if cursor.is_empty() {
            None
        } else {
            Some(cursor)
        };
        Ok((out, next))
    }
}
