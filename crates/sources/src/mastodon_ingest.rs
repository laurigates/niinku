//! Live Mastodon ingestion.
//!
//! Pulls posts from one or more hashtag-timeline streams on an
//! instance, filters to a target language, tokenises each post's
//! `content` into a [`FreqTable`], and discards raw text immediately —
//! only aggregate counts ever leave this function.
//!
//! Hashtag streams are used in preference to `/timelines/public`
//! because modern Mastodon (4.x+) requires authentication for the
//! public timeline, while `/timelines/tag/:hashtag` remains
//! unauthenticated on most instances.
//!
//! Pagination uses the response's `Link: …; rel="next"` header, with
//! a `max_id` fallback derived from the oldest post in each page when
//! the header is missing.
//!
//! No async runtime — `ureq` is sync and blocking, matching the rest
//! of the pipeline.

use anyhow::{Context, Result};
use niinku_pipeline::{Count, FreqTable};
use serde::Deserialize;

use crate::tokenize::{strip_html, tokenize_for_freq};

/// A single Mastodon status — only the fields we actually read.
/// `language` may be missing or null; the API still includes the post.
#[derive(Debug, Deserialize)]
struct Status {
    id: String,
    content: String,
    #[serde(default)]
    language: Option<String>,
}

/// Pull posts from `instance`'s hashtag timelines for each tag in
/// `tags`, filter to `language` (e.g. `"fi"`), and accumulate token
/// counts into a single [`FreqTable`].
///
/// `target_posts` is the total budget shared across all tags. Modern
/// Mastodon requires authentication for `/timelines/public`, but
/// `/timelines/tag/:hashtag` remains unauthenticated on most instances,
/// so we use hashtag streams as the canonical source.
///
/// `progress` is called after each page with `(fetched_so_far,
/// target_posts)` so callers can show a status line.
pub fn fetch_and_count(
    instance: &str,
    language: &str,
    tags: &[String],
    target_posts: usize,
    mut progress: impl FnMut(usize, usize),
) -> Result<FreqTable> {
    let agent = ureq::AgentBuilder::new()
        .user_agent(concat!("niinku/", env!("CARGO_PKG_VERSION")))
        .build();

    let mut table = FreqTable::new();
    let mut fetched = 0usize;

    for tag in tags {
        let base = format!("https://{instance}/api/v1/timelines/tag/{tag}");
        let mut next_url: Option<String> = Some(format!("{base}?limit=40"));

        while let Some(url) = next_url.take() {
            let resp = agent
                .get(&url)
                .call()
                .with_context(|| format!("GET {url}"))?;

            let link_header = resp.header("Link").map(str::to_string);
            let body = resp
                .into_string()
                .with_context(|| format!("reading body from {url}"))?;
            let posts: Vec<Status> =
                serde_json::from_str(&body).with_context(|| format!("parsing JSON from {url}"))?;

            if posts.is_empty() {
                break;
            }

            for post in &posts {
                if !matches!(&post.language, Some(l) if l == language) {
                    continue;
                }
                let text = strip_html(&post.content);
                for tok in tokenize_for_freq(&text) {
                    table.insert(tok, 1);
                }
            }

            fetched += posts.len();
            progress(fetched, target_posts);

            if fetched >= target_posts {
                return Ok(table);
            }

            next_url = next_link(link_header.as_deref()).or_else(|| {
                posts
                    .last()
                    .map(|s| format!("{base}?limit=40&max_id={}", s.id))
            });
        }
    }

    Ok(table)
}

/// Parse the Mastodon-style `Link: <url>; rel="next", <url>; rel="prev"`
/// header and return the URL marked `rel="next"`, if any.
fn next_link(header: Option<&str>) -> Option<String> {
    let h = header?;
    for part in h.split(',') {
        let part = part.trim();
        // Expected shape: `<https://...>; rel="next"`
        let (url_part, rel_part) = part.split_once(';')?;
        let url = url_part
            .trim()
            .trim_start_matches('<')
            .trim_end_matches('>');
        let rel = rel_part.trim();
        if rel == "rel=\"next\"" {
            return Some(url.to_string());
        }
    }
    None
}

/// Serialise a [`FreqTable`] as `word count\n` lines, sorted by count
/// descending then by word ascending so reruns are diff-stable.
pub fn write_freq_table(table: &FreqTable, w: &mut impl std::io::Write) -> std::io::Result<()> {
    let mut pairs: Vec<(&String, Count)> = table.iter().map(|(k, v)| (k, *v)).collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (word, count) in pairs {
        writeln!(w, "{word} {count}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_link_extracts_rel_next() {
        let header = r#"<https://example.com/api/v1/timelines/public?max_id=42>; rel="next", <https://example.com/api/v1/timelines/public?min_id=99>; rel="prev""#;
        assert_eq!(
            next_link(Some(header)),
            Some("https://example.com/api/v1/timelines/public?max_id=42".to_string())
        );
    }

    #[test]
    fn next_link_none_when_header_missing() {
        assert_eq!(next_link(None), None);
    }

    #[test]
    fn next_link_none_when_only_prev() {
        let header = r#"<https://example.com/api/v1/timelines/public?min_id=99>; rel="prev""#;
        assert_eq!(next_link(Some(header)), None);
    }

    #[test]
    fn write_freq_table_is_sorted_desc() {
        let t = FreqTable::from_pairs([("rare", 1u64), ("common", 100), ("medium", 20)]);
        let mut buf = Vec::new();
        write_freq_table(&t, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s, "common 100\nmedium 20\nrare 1\n");
    }
}
