//! Pure text utilities shared by the live-source adapters.
//!
//! Kept separate from the network code so the tokenisation pipeline can
//! be unit-tested without any HTTP traffic.

use unicode_segmentation::UnicodeSegmentation;

/// Strip simple HTML tags and decode the entities Mastodon's server-side
/// sanitiser actually emits. Not a general-purpose HTML parser — good
/// enough for `<p>…</p>` / `<br/>` / `<a ...>…</a>` Mastodon content.
pub fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match (c, in_tag) {
            ('<', _) => in_tag = true,
            ('>', _) => in_tag = false,
            (_, false) => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Drop whitespace-delimited tokens that look like @mentions, #hashtags,
/// or URLs before word-segmentation. `unicode_words` strips the leading
/// `@`/`#` on its own, so without this step `@alice` would land in the
/// frequency table as `alice`.
fn strip_handles_and_urls(text: &str) -> String {
    text.split_whitespace()
        .filter(|t| {
            !t.starts_with('@')
                && !t.starts_with('#')
                && !t.starts_with("http://")
                && !t.starts_with("https://")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Word-segment `text` into lowercased frequency tokens. Drops mentions,
/// hashtags, URLs, single-character tokens, and pure-numeric tokens.
/// Does *not* try to detect language — that's the downstream Voikko
/// filter's job.
pub fn tokenize_for_freq(text: &str) -> Vec<String> {
    let cleaned = strip_handles_and_urls(text);
    cleaned
        .unicode_words()
        .map(|w| w.to_lowercase())
        .filter(|w| w.chars().count() >= 2)
        // Drop number-only tokens including decimals like "3.14".
        .filter(|w| w.chars().any(|c| c.is_alphabetic()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_removes_tags_and_decodes_entities() {
        let input = "<p>Moi <a href=\"x\">kaverit</a>!&nbsp;Tää on &quot;hyvä&quot;.</p>";
        let out = strip_html(input);
        assert_eq!(out, "Moi kaverit! Tää on \"hyvä\".");
    }

    #[test]
    fn strip_html_handles_br_and_nested_tags() {
        let input = "<p>rivi 1<br/>rivi 2</p>";
        assert_eq!(strip_html(input), "rivi 1rivi 2");
    }

    #[test]
    fn tokenize_lowercases_and_segments_finnish() {
        let toks = tokenize_for_freq("Moi! Tää on hauska päivä.");
        assert_eq!(toks, vec!["moi", "tää", "on", "hauska", "päivä"]);
    }

    #[test]
    fn tokenize_drops_mentions_hashtags_urls() {
        let toks = tokenize_for_freq("@alice moro #suomi https://example.com terve");
        assert_eq!(toks, vec!["moro", "terve"]);
    }

    #[test]
    fn tokenize_drops_single_chars_and_pure_numbers() {
        let toks = tokenize_for_freq("a b 12 ok mä 3.14 ää");
        // "3.14" splits to "3" and "14" — both pure numeric, dropped.
        // "a"/"b" single-char, dropped. "ok"/"mä"/"ää" kept.
        assert_eq!(toks, vec!["ok", "mä", "ää"]);
    }

    #[test]
    fn full_pipeline_html_to_tokens() {
        let html = "<p>Moi <a href=\"https://x.fi\">@kaveri</a> niinku tää on aika hauska 😄</p>";
        let toks = tokenize_for_freq(&strip_html(html));
        // @kaveri stripped (mention); URL stripped; "Moi" lowercased;
        // emoji is unicode_words-classified as non-word, dropped.
        assert_eq!(toks, vec!["moi", "niinku", "tää", "on", "aika", "hauska"]);
    }
}
