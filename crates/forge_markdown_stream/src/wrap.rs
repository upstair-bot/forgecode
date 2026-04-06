//! Text wrapping with space-preserving semantics for Korean/Hangul and other
//! space-delimited scripts.
//!
//! This module provides [`wrap_with_prefixes`] as a local replacement for
//! `streamdown_render::text::text_wrap` in list-item and blockquote rendering
//! paths. The upstream `text_wrap` treats Hangul (and other CJK Unicode
//! ranges) as no-space text, silently dropping the spaces that separate Korean
//! words. This implementation only breaks at whitespace characters, which
//! preserves Korean spacing exactly as the author wrote it.
//!
//! # Width semantics
//!
//! The `width` parameter means the **maximum visible content width per line**,
//! matching the convention of the upstream `text_wrap` function. Prefixes are
//! prepended to each line but do **not** reduce the content budget.

use streamdown_ansi::utils::visible_length;
use unicode_width::UnicodeWidthChar;

/// Wrap ANSI-encoded `text` into terminal lines with prefix support.
///
/// Lines are broken **only at whitespace boundaries**, so spaces adjacent to
/// Korean (Hangul) syllables are preserved rather than dropped.
///
/// # Arguments
/// - `text`: source text; may contain CSI/OSC ANSI escape sequences.
/// - `width`: maximum visible content width per line. The prefix is prepended
///   to each output line but does **not** count against this budget, matching
///   the convention of `streamdown_render::text::text_wrap`.
/// - `first_prefix`: prepended verbatim to the first output line.
/// - `next_prefix`: prepended verbatim to every subsequent output line.
///
/// Returns an empty `Vec` when `text` is empty.
#[allow(unused_assignments)] // macro-expanded resets are intentional hand-offs
pub fn wrap_with_prefixes(
    text: &str,
    width: usize,
    first_prefix: &str,
    next_prefix: &str,
) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }

    // Fast path: content fits on one line.
    if width == 0 || visible_length(text) <= width {
        return vec![format!("{}{}", first_prefix, text)];
    }

    let mut lines: Vec<String> = Vec::new();
    // Accumulated content for the current line (no prefix).
    let mut line = String::new();
    let mut line_width: usize = 0;
    // Current word being assembled between whitespace boundaries.
    let mut word = String::new();
    let mut word_width: usize = 0;
    // Last active ANSI colour/style sequence for re-applying after line breaks.
    let mut active_style: Option<String> = None;
    let mut is_first = true;

    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    // Push the accumulated `line` as a complete output line and reset state.
    macro_rules! push_line {
        () => {{
            let prefix = if is_first { first_prefix } else { next_prefix };
            let mut out = prefix.to_string();
            out.push_str(&line);
            if active_style.is_some() {
                out.push_str("\x1b[0m");
            }
            lines.push(out);
            // Begin next line pre-loaded with any open ANSI style.
            line = active_style.clone().unwrap_or_default();
            line_width = 0;
            is_first = false;
        }};
    }

    while i < chars.len() {
        let c = chars[i];

        // ── ANSI escape sequence (zero visible width) ─────────────────────
        if c == '\x1b' {
            let mut esc = String::from('\x1b');
            i += 1;

            if i < chars.len() {
                let next = chars[i];
                esc.push(next);
                i += 1;

                if next == '[' {
                    // CSI — read until alphabetic terminator.
                    while i < chars.len() {
                        let sc = chars[i];
                        esc.push(sc);
                        i += 1;
                        if sc == 'm' || sc == 'K' || sc == 'H' || sc == 'J' {
                            break;
                        }
                    }
                    if esc.ends_with('m') {
                        active_style = if esc == "\x1b[0m" {
                            None
                        } else {
                            Some(esc.clone())
                        };
                    }
                } else if next == ']' {
                    // OSC — read until ST (\x1b\\) or BEL (\x07).
                    while i < chars.len() {
                        let sc = chars[i];
                        esc.push(sc);
                        i += 1;
                        if sc == '\x07' {
                            break;
                        }
                        if sc == '\\' && esc.len() >= 2 {
                            if esc.chars().rev().nth(1) == Some('\x1b') {
                                break;
                            }
                        }
                    }
                }
            }

            // ANSI sequences carry zero visible width; keep them in the word
            // so they stay attached to the correct text token.
            word.push_str(&esc);
            continue;
        }

        // ── Whitespace — the only valid word-break point ──────────────────
        let cw = c.width().unwrap_or(0);

        if c.is_whitespace() {
            if line_width + word_width + cw > width && line_width > 0 {
                // Word + space does not fit: end current line and begin the
                // next with word + space (same behaviour as table.rs).
                push_line!();
                line.push_str(&word);
                line.push(c);
                line_width = word_width + cw;
            } else {
                line.push_str(&word);
                line.push(c);
                line_width += word_width + cw;
            }
            word.clear();
            word_width = 0;
        } else {
            // ── Normal character (ASCII, Hangul, CJK, …) ─────────────────
            word.push(c);
            word_width += cw;

            // If the single word exceeds `width`, break it by character
            // (handles URLs or other unspaced tokens longer than the width).
            if word_width > width {
                if line_width > 0 {
                    // Push current content before the oversized word.
                    let prefix = if is_first { first_prefix } else { next_prefix };
                    let mut out = prefix.to_string();
                    out.push_str(&line);
                    if active_style.is_some() {
                        out.push_str("\x1b[0m");
                    }
                    lines.push(out);
                    is_first = false;
                    line = active_style.clone().unwrap_or_default();
                    line_width = 0;
                }

                // Split the oversized word into width-sized chunks.
                while visible_length(&word) > width {
                    let (chunk, rest) = split_at_visible_width(&word, width);
                    let prefix = if is_first { first_prefix } else { next_prefix };
                    let mut out = prefix.to_string();
                    if let Some(ref s) = active_style {
                        out.push_str(s);
                    }
                    out.push_str(&chunk);
                    if active_style.is_some() {
                        out.push_str("\x1b[0m");
                    }
                    lines.push(out);
                    is_first = false;
                    word = rest;
                }
                line = active_style.clone().unwrap_or_default();
                line.push_str(&word);
                line_width = visible_length(&word);
                word.clear();
                word_width = 0;
            }
        }

        i += 1;
    }

    // Flush the remaining word.
    if !word.is_empty() {
        if line_width + word_width > width && line_width > 0 {
            push_line!();
            line.push_str(&word);
        } else {
            line.push_str(&word);
        }
    }

    // Emit the final line (always; even if content is empty when there were
    // prior lines that reset `is_first`).
    let prefix = if is_first { first_prefix } else { next_prefix };
    let mut out = prefix.to_string();
    out.push_str(&line);
    lines.push(out);

    lines
}

/// Split `word` (which may contain ANSI sequences) at the first position where
/// the visible width would exceed `width`, returning `(head, tail)`.
fn split_at_visible_width(word: &str, width: usize) -> (String, String) {
    let mut head = String::new();
    let mut tail = String::new();
    let mut head_w = 0usize;
    let mut in_head = true;

    let chars: Vec<char> = word.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c == '\x1b' {
            let mut esc = String::from('\x1b');
            i += 1;
            if i < chars.len() {
                let next = chars[i];
                esc.push(next);
                i += 1;
                if next == '[' {
                    while i < chars.len() {
                        let sc = chars[i];
                        esc.push(sc);
                        i += 1;
                        if sc == 'm' || sc == 'K' || sc == 'H' || sc == 'J' {
                            break;
                        }
                    }
                } else if next == ']' {
                    while i < chars.len() {
                        let sc = chars[i];
                        esc.push(sc);
                        i += 1;
                        if sc == '\x07' {
                            break;
                        }
                        if sc == '\\' && esc.len() >= 2 {
                            if esc.chars().rev().nth(1) == Some('\x1b') {
                                break;
                            }
                        }
                    }
                }
            }
            if in_head {
                head.push_str(&esc);
            } else {
                tail.push_str(&esc);
            }
            continue;
        }

        if in_head {
            let cw = c.width().unwrap_or(0);
            if head_w + cw <= width {
                head.push(c);
                head_w += cw;
            } else {
                tail.push(c);
                in_head = false;
            }
        } else {
            tail.push(c);
        }

        i += 1;
    }

    (head, tail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Strip ANSI codes for plain-text comparison.
    fn strip(s: &str) -> String {
        String::from_utf8(strip_ansi_escapes::strip(s)).unwrap()
    }

    fn wrap(text: &str, width: usize, fp: &str, np: &str) -> Vec<String> {
        wrap_with_prefixes(text, width, fp, np)
    }

    // ── Basic ASCII ────────────────────────────────────────────────────────

    #[test]
    fn empty_text_returns_empty_vec() {
        let actual = wrap("", 80, "  • ", "    ");
        let expected: Vec<String> = vec![];
        assert_eq!(actual, expected);
    }

    #[test]
    fn short_text_fits_on_first_line() {
        // width=80 content cols; "hello world" = 11 cols → fits
        let actual = wrap("hello world", 80, "  • ", "    ");
        let expected = vec!["  • hello world"];
        assert_eq!(actual, expected);
    }

    #[test]
    fn ascii_wraps_at_space() {
        // width=11 content cols (prefix not counted).
        // "hello " = 6, adding "world " = 6+6=12 > 11 → wrap before "world"
        let actual = wrap("hello world foo", 11, "  • ", "    ");
        let stripped: Vec<String> = actual.iter().map(|l| strip(l)).collect();
        assert!(stripped.len() >= 2, "should wrap: {:?}", stripped);
        assert!(
            stripped[0].contains("hello"),
            "first line must contain 'hello'"
        );
        assert!(
            actual.iter().any(|l| strip(l).contains("world")),
            "some line must contain 'world'"
        );
    }

    // ── Korean spacing preservation ────────────────────────────────────────

    #[test]
    fn korean_spaces_preserved_no_wrap_needed() {
        // width=80; "안녕 하세요" = 11 cols → fits on one line; space preserved.
        let text = "안녕 하세요";
        let actual = wrap(text, 80, "  • ", "    ");
        assert_eq!(actual.len(), 1, "should be 1 line: {:?}", actual);
        assert!(
            actual[0].contains("안녕 하세요"),
            "space must be preserved: {:?}",
            actual[0]
        );
    }

    #[test]
    fn korean_spaces_preserved_after_wrap() {
        // width=8 content cols (prefix not counted).
        // "안녕" = 4 cols, space = 1 → "안녕 " = 5 ≤ 8, fits.
        // "하세요" = 6 cols: 5+6 = 11 > 8 → wrap.
        let text = "안녕 하세요";
        let actual = wrap(text, 8, "  • ", "    ");
        assert_eq!(actual.len(), 2, "expected 2 lines: {:?}", actual);
        // First line must contain "안녕 " WITH the trailing space.
        assert!(
            actual[0].contains("안녕 "),
            "space after 안녕 must be preserved: {:?}",
            actual[0]
        );
        // Second line must contain "하세요".
        assert!(
            actual[1].contains("하세요"),
            "second line must contain 하세요: {:?}",
            actual[1]
        );
    }

    #[test]
    fn korean_multi_word_spaces_all_preserved() {
        // width=80; all spaces in a multi-word Korean sentence survive.
        let text = "이것은 한국어 테스트 문장입니다";
        let actual = wrap(text, 80, "  • ", "    ");
        assert!(
            actual[0].contains("이것은 한국어 테스트 문장입니다"),
            "all spaces must be preserved: {:?}",
            actual[0]
        );
    }

    #[test]
    fn korean_wraps_preserving_all_spaces() {
        // width=14 content cols forces multi-line.
        // "이것은 " = 3*2+1 = 7 ≤ 14. "한국어 " = 3*2+1 = 7: 7+7=14 ≤ 14 → fits.
        // "테스트" = 3*2 = 6: 14+6=20 > 14 → wrap after "한국어 ".
        let text = "이것은 한국어 테스트";
        let actual = wrap(text, 14, "  • ", "    ");
        assert!(actual.len() >= 2, "expected wrapping: {:?}", actual);
        // All Korean words must be present across lines.
        let all = actual.join("");
        assert!(all.contains("이것은"), "이것은 missing: {:?}", all);
        assert!(all.contains("한국어"), "한국어 missing: {:?}", all);
        assert!(all.contains("테스트"), "테스트 missing: {:?}", all);
    }

    // ── Continuation prefix ────────────────────────────────────────────────

    #[test]
    fn continuation_lines_use_next_prefix() {
        // width=7 content cols to force a wrap.
        let text = "word1 word2 word3";
        let actual = wrap(text, 7, "A> ", "   ");
        assert!(
            actual[0].starts_with("A> "),
            "first line prefix: {:?}",
            actual[0]
        );
        if actual.len() > 1 {
            assert!(
                actual[1].starts_with("   "),
                "cont. prefix: {:?}",
                actual[1]
            );
        }
    }

    // ── ANSI ───────────────────────────────────────────────────────────────

    #[test]
    fn ansi_codes_do_not_affect_width_measurement() {
        // Bold "hello" + " world" — visually same as plain.
        let bold_hello = "\x1b[1mhello\x1b[0m";
        let text = format!("{} world", bold_hello);
        let actual = wrap(&text, 80, "  ", "  ");
        // Should fit on one line with width=80.
        assert_eq!(actual.len(), 1);
        assert!(actual[0].contains("hello"));
        assert!(actual[0].contains("world"));
    }
}
