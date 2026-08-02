//! Extract a session credential pair from a browser "Copy as cURL" command.
//!
//! DevTools emits the whole authenticated request, so one clipboard copy
//! carries both halves: the `xoxc` token (form body or JSON payload) and the
//! `d` cookie (Cookie header or `-b` flag). This exists because the `d` cookie
//! is HttpOnly — no page script can read it, so a console one-liner cannot
//! produce it.

use crate::error::{Error, Result};

/// Token and cookie recovered from a pasted cURL command.
#[derive(Debug)]
pub struct Pair {
    pub token: String,
    pub cookie: String,
}

/// Scan a pasted cURL command for an `xoxc-` token and an `xoxd-` cookie.
///
/// Deliberately format-agnostic: browsers differ in quoting, line
/// continuations, and whether the token arrives as multipart form data, a
/// urlencoded body, or JSON. Both values have unambiguous prefixes, so
/// scanning for those beats parsing shell syntax.
pub fn parse(input: &str) -> Result<Pair> {
    let token = find_prefixed(input, "xoxc-").ok_or_else(|| {
        Error::Usage(
            "no xoxc- token found in the pasted command — copy a request to /api/ while signed in"
                .into(),
        )
    })?;
    let cookie = find_prefixed(input, "xoxd-").ok_or_else(|| {
        Error::Usage(
            "no xoxd- cookie found in the pasted command — make sure you used \
             'Copy as cURL' (not 'Copy as fetch', which omits cookies)"
                .into(),
        )
    })?;
    Ok(Pair { token, cookie })
}

/// Take the longest run of credential-safe characters starting at `prefix`.
///
/// Stops at any quote, whitespace, `;`, `&`, or `,` — the delimiters that
/// separate a token from surrounding shell/HTTP syntax. `%`, `+`, `/`, `=`,
/// `-` are kept because the cookie arrives URL-encoded and must stay verbatim.
fn find_prefixed(haystack: &str, prefix: &str) -> Option<String> {
    let start = haystack.find(prefix)?;
    let tail = &haystack[start..];
    let end = tail
        .find(|c: char| {
            !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '%' | '+' | '/' | '=' | '.'))
        })
        .unwrap_or(tail.len());
    let value = tail[..end].trim_end_matches('=').to_string();
    if value.len() <= prefix.len() {
        return None;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chrome_multipart_form() {
        let input = r#"curl 'https://acme.slack.com/api/conversations.history' \
  -H 'cookie: b=abc; d=xoxd-AbC%2F123%2BdEf%3D; d-s=1754160000; lc=1754160000' \
  --data-raw $'------WebKitFormBoundary\r\nContent-Disposition: form-data; name="token"\r\n\r\nxoxc-1234-5678-abcdef\r\n------WebKitFormBoundary--\r\n'"#;
        let p = parse(input).expect("should parse");
        assert_eq!(p.token, "xoxc-1234-5678-abcdef");
        assert_eq!(p.cookie, "xoxd-AbC%2F123%2BdEf%3D");
    }

    #[test]
    fn parses_firefox_style_b_flag_and_json_body() {
        let input = r#"curl "https://acme.slack.com/api/auth.test" -X POST \
  -b "d=xoxd-zzz999%2F; d-s=1" \
  --data '{"token":"xoxc-9999-0000-ffff"}'"#;
        let p = parse(input).expect("should parse");
        assert_eq!(p.token, "xoxc-9999-0000-ffff");
        assert_eq!(p.cookie, "xoxd-zzz999%2F");
    }

    #[test]
    fn cookie_stays_url_encoded() {
        let input = "token=xoxc-a1 cookie: d=xoxd-p%2Bq%2Fr%3D;";
        let p = parse(input).expect("should parse");
        assert_eq!(p.cookie, "xoxd-p%2Bq%2Fr%3D");
    }

    #[test]
    fn missing_cookie_is_an_error() {
        let err = parse("--data 'token=xoxc-abc123'").unwrap_err();
        assert!(err.to_string().contains("no xoxd- cookie"));
    }

    #[test]
    fn missing_token_is_an_error() {
        let err = parse("-b 'd=xoxd-abc123'").unwrap_err();
        assert!(err.to_string().contains("no xoxc- token"));
    }
}
