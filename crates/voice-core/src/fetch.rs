//! Fetch-a-URL for the add-writing flow: bounded, fail-closed HTTP(S)
//! retrieval with a naive HTML-to-text reduction. Only the URL Matt
//! explicitly provides is fetched; redirects are capped; the body is
//! size-bounded before any parsing; the result is plain text data —
//! markup and page instructions are stripped or treated as text.

use std::io::Read;
use std::time::Duration;

use crate::error::{Result, VoiceError};
use crate::{FETCH_MAX_BYTES, FETCH_TIMEOUT_SECONDS};

/// Maximum accepted URL length.
pub const URL_MAX_CHARS: usize = 2048;

/// Fetch `url` and return its text content (HTML reduced to text).
///
/// Rejects non-HTTP(S) schemes, oversized bodies, and non-2xx statuses.
pub fn fetch_url_text(url: &str) -> Result<String> {
    let url = url.trim();
    if url.is_empty() || url.chars().count() > URL_MAX_CHARS {
        return Err(VoiceError::invalid("url", "empty or overlong URL"));
    }
    let lowered = url.to_ascii_lowercase();
    if !lowered.starts_with("http://") && !lowered.starts_with("https://") {
        return Err(VoiceError::invalid(
            "url",
            "only http:// and https:// URLs are fetched",
        ));
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECONDS))
        .redirects(3)
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|e| VoiceError::Fetch(e.to_string()))?;
    let mut reader = response
        .into_reader()
        .take(u64::try_from(FETCH_MAX_BYTES + 1).unwrap_or(u64::MAX));
    let mut body = Vec::new();
    reader
        .read_to_end(&mut body)
        .map_err(|e| VoiceError::Fetch(e.to_string()))?;
    if body.len() > FETCH_MAX_BYTES {
        return Err(VoiceError::Fetch(format!(
            "body exceeds {FETCH_MAX_BYTES} bytes"
        )));
    }
    let text = String::from_utf8_lossy(&body).into_owned();
    if looks_like_html(&text) {
        Ok(html_to_text(&text))
    } else {
        Ok(text)
    }
}

fn looks_like_html(text: &str) -> bool {
    let head = text.to_ascii_lowercase();
    head.contains("<html") || head.contains("<body") || head.contains("<!doctype html")
}

/// Naive, bounded HTML → text: drop script/style blocks, turn block
/// tags into newlines, strip remaining tags, decode common entities,
/// collapse blank-line runs. Not a parser — a reduction for prose
/// ingestion, with all loops bounded by the input length.
pub fn html_to_text(html: &str) -> String {
    let without_blocks = strip_blocks(html, "script");
    let without_blocks = strip_blocks(&without_blocks, "style");
    let chars: Vec<char> = without_blocks.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '<' {
            let mut j = i + 1;
            let mut tag = String::new();
            while j < chars.len() && chars[j] != '>' && tag.len() < 64 {
                tag.push(chars[j]);
                j += 1;
            }
            let tag_name: String = tag
                .trim_start_matches('/')
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase();
            if matches!(
                tag_name.as_str(),
                "p" | "div"
                    | "br"
                    | "li"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "tr"
                    | "section"
                    | "article"
            ) {
                out.push('\n');
            } else {
                out.push(' ');
            }
            i = if j < chars.len() { j + 1 } else { j };
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'");
    let mut lines: Vec<String> = Vec::new();
    let mut blank_run = 0usize;
    for line in decoded.lines() {
        let line = line.trim();
        if line.is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                lines.push(String::new());
            }
        } else {
            blank_run = 0;
            lines.push(collapse_spaces(line));
        }
    }
    lines.join("\n").trim().to_string()
}

fn strip_blocks(html: &str, tag: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut rest_lower = lower.as_str();
    loop {
        match rest_lower.find(&open) {
            None => {
                out.push_str(rest);
                break;
            }
            Some(start) => {
                out.push_str(&rest[..start]);
                match rest_lower[start..].find(&close) {
                    Some(end_rel) => {
                        let end = start + end_rel + close.len();
                        rest = &rest[end..];
                        rest_lower = &rest_lower[end..];
                    }
                    None => break, // unterminated block: drop the tail
                }
            }
        }
    }
    out
}

fn collapse_spaces(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_ws = false;
    for c in line.chars() {
        if c.is_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_reduction_strips_scripts_and_tags() {
        let html = "<html><head><style>.x{}</style><script>alert(1)</script></head>\
                    <body><h1>Title</h1><p>First &amp; foremost.</p><p>Second para.</p></body></html>";
        let text = html_to_text(html);
        assert!(text.contains("Title"));
        assert!(text.contains("First & foremost."));
        assert!(!text.contains("alert"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert!(fetch_url_text("file:///etc/passwd").is_err());
        assert!(fetch_url_text("ftp://example.org/x").is_err());
        assert!(fetch_url_text("").is_err());
    }
}
