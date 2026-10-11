//! Deterministic stylometry — a line-for-line port of the matt-voice
//! skill's `scripts/stylometry.py` (pure counting, no LLM calls, no
//! randomness: same input, same numbers).
//!
//! Parity notes (measured, not hidden):
//! - Rounding mirrors Python's `round(x, dp)` (half-even) at exactly the
//!   points the Python code rounds: per-1k rates to 2 dp, sentence stats
//!   to 2 dp, paragraph stats to 1 dp, type-token ratio to 3 dp.
//! - Python's `statistics.mean`/`pstdev` use exact rational arithmetic;
//!   this port sums in f64. The difference is below 1e-9 relative —
//!   invisible after the 2–3 dp rounding both implementations apply.
//! - Python regex `\s` and `str.split()` are Unicode-aware; this port
//!   uses `char::is_whitespace`, which covers the same whitespace set
//!   for every input in the corpus.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::lists::{ASSERTIVES, FUNCTION_WORDS, HEDGES};

/// Punctuation rates per 1,000 words. Field names match the Python
/// fingerprint JSON exactly (the parity fixture compares them by name).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PunctRates {
    /// Em dashes (plus the ` -- ` ASCII form) per 1k words.
    pub em_dash: f64,
    /// Semicolons per 1k words.
    pub semicolon: f64,
    /// Colons per 1k words.
    pub colon: f64,
    /// Commas per 1k words.
    pub comma: f64,
    /// Opening parentheses per 1k words.
    pub paren_open: f64,
    /// Question marks per 1k words.
    pub question: f64,
    /// Exclamation marks per 1k words.
    pub exclaim: f64,
    /// Hyphenated compounds (`word-word`) per 1k words.
    pub hyphenated_compounds: f64,
}

/// One text's measured fingerprint. Field names match the Python
/// `stylometry.fingerprint()` output keys exactly.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Fingerprint {
    /// Token count (see [`tokenize`]).
    pub words: usize,
    /// Sentence count (sentences of >= 2 whitespace-separated chunks).
    pub sentences: usize,
    /// Mean sentence length in tokens.
    pub sent_len_mean: f64,
    /// Median sentence length in tokens.
    pub sent_len_median: f64,
    /// Population stdev of sentence length.
    pub sent_len_stdev: f64,
    /// Percent of sentences with <= 10 tokens.
    pub sent_pct_short_le10: f64,
    /// Percent of sentences with >= 30 tokens.
    pub sent_pct_long_ge30: f64,
    /// Mean paragraph length in tokens.
    pub para_len_mean: f64,
    /// Median paragraph length in tokens.
    pub para_len_median: f64,
    /// Every function word's rate per 1k tokens.
    pub function_words_per1k: BTreeMap<String, f64>,
    /// Punctuation rates per 1k tokens.
    pub punct_per1k: PunctRates,
    /// Unique tokens / tokens.
    pub type_token_ratio: f64,
    /// Percent of tokens with >= 3 syllables.
    pub pct_words_3plus_syll: f64,
    /// Hedge-word rate per 1k tokens.
    pub hedges_per1k: f64,
    /// Assertive-word rate per 1k tokens.
    pub assertives_per1k: f64,
    /// First-person rate (i/we/our/us) per 1k tokens.
    pub first_person_per1k: f64,
    /// `we` rate per 1k tokens.
    pub we_per1k: f64,
    /// Top content words: (word, count), count desc, first-seen breaks
    /// ties — Python `Counter.most_common` order.
    pub top_content_words: Vec<(String, u64)>,
}

/// Round half-even at `dp` decimal places, mirroring Python `round`.
pub(crate) fn round_dp(x: f64, dp: u32) -> f64 {
    let scale = 10f64.powi(i32::try_from(dp).unwrap_or(0));
    (x * scale).round_ties_even() / scale
}

/// Ligature and quote normalization, in the Python dict's order.
fn normalize(text: &str) -> String {
    text.replace('\u{FB01}', "fi")
        .replace('\u{FB02}', "fl")
        .replace('\u{FB00}', "ff")
        .replace('\u{FB03}', "ffi")
        .replace('\u{FB04}', "ffl")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'")
        .replace('\u{201C}', "\"")
        .replace('\u{201D}', "\"")
        .replace('\u{2013}', "-")
}

/// Tokenize exactly as Python `re.findall(r"[a-z]+(?:'[a-z]+)?",
/// t.lower())`: lowercase first, then maximal a-z runs with at most one
/// internal apostrophe-plus-letters continuation.
pub fn tokenize(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i].is_ascii_lowercase() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_lowercase() {
                i += 1;
            }
            if i + 1 < chars.len() && chars[i] == '\'' && chars[i + 1].is_ascii_lowercase() {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_lowercase() {
                    i += 1;
                }
            }
            tokens.push(chars[start..i].iter().collect());
        } else {
            i += 1;
        }
    }
    tokens
}

/// Sentence split mirroring Python: collapse whitespace runs to single
/// spaces, split after [.!?] where the next chunk starts with
/// [A-Z0-9"“], keep sentences of >= 2 whitespace-separated chunks.
fn sentences(text: &str) -> Vec<String> {
    let mut collapsed = String::with_capacity(text.len());
    let mut in_ws = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !in_ws {
                collapsed.push(' ');
                in_ws = true;
            }
        } else {
            collapsed.push(c);
            in_ws = false;
        }
    }
    let chars: Vec<char> = collapsed.chars().collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == ' '
            && i > 0
            && matches!(chars[i - 1], '.' | '!' | '?')
            && i + 1 < chars.len()
            && (chars[i + 1].is_ascii_uppercase()
                || chars[i + 1].is_ascii_digit()
                || chars[i + 1] == '"'
                || chars[i + 1] == '\u{201C}')
        {
            push_sentence(&chars[start..i], &mut out);
            start = i + 1;
            i += 1;
        }
        i += 1;
    }
    push_sentence(&chars[start..], &mut out);
    out
}

fn push_sentence(chunk: &[char], out: &mut Vec<String>) {
    let s: String = chunk.iter().collect();
    let trimmed = s.trim();
    if trimmed.split_whitespace().count() >= 2 {
        out.push(trimmed.to_string());
    }
}

/// Paragraphs mirroring Python `re.split(r'\n\s*\n', text)`: split on
/// maximal whitespace runs containing at least two newlines; keep
/// paragraphs of >= 3 whitespace-separated chunks. (Split points inside
/// a pure-whitespace run cannot change the resulting token counts.)
pub fn paragraphs(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            let run_start = i;
            let mut newlines = 0usize;
            while i < chars.len() && chars[i].is_whitespace() {
                if chars[i] == '\n' {
                    newlines += 1;
                }
                i += 1;
            }
            if newlines >= 2 {
                push_paragraph(&chars[start..run_start], &mut out);
                start = i;
            }
        } else {
            i += 1;
        }
    }
    push_paragraph(&chars[start..], &mut out);
    out
}

fn push_paragraph(chunk: &[char], out: &mut Vec<String>) {
    let s: String = chunk.iter().collect();
    if s.split_whitespace().count() >= 3 {
        out.push(s);
    }
}

/// Syllable estimate, mirroring the Python heuristic exactly.
fn syllables(word: &str) -> usize {
    let w: String = word.chars().filter(|c| c.is_ascii_lowercase()).collect();
    if w.len() <= 3 {
        return 1;
    }
    let mut n = 0usize;
    let mut in_vowel = false;
    for c in w.chars() {
        let vowel = matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
        if vowel && !in_vowel {
            n += 1;
        }
        in_vowel = vowel;
    }
    let mut count = i64::try_from(n).unwrap_or(0);
    if w.ends_with('e') && !w.ends_with("le") && !w.ends_with("ee") && !w.ends_with("ye") {
        count -= 1;
    }
    usize::try_from(count.max(1)).unwrap_or(1)
}

/// Count non-overlapping `[A-Za-z]+-[A-Za-z]+` matches (Python
/// `re.findall` semantics on the normalized, non-lowercased text).
fn count_hyphenated(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut count = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i].is_ascii_alphabetic() {
            let run_start = i;
            while i < chars.len() && chars[i].is_ascii_alphabetic() {
                i += 1;
            }
            if i + 1 < chars.len()
                && chars[i] == '-'
                && chars[i + 1].is_ascii_alphabetic()
                && i > run_start
            {
                count += 1;
                i += 1; // the hyphen
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }
    count
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / f64::from(u32::try_from(values.len()).unwrap_or(u32::MAX))
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    }
}

fn pstdev(values: &[f64]) -> f64 {
    if values.len() <= 1 {
        return 0.0;
    }
    let m = mean(values);
    let var = values.iter().map(|v| (v - m) * (v - m)).sum::<f64>()
        / f64::from(u32::try_from(values.len()).unwrap_or(u32::MAX));
    var.sqrt()
}

/// Measure one text. Empty-after-tokenization input yields the zero
/// fingerprint (`words == 0`), mirroring Python's `{}` return.
pub fn measure(text: &str) -> Fingerprint {
    let text = normalize(text);
    let toks = tokenize(&text);
    let n = toks.len();
    if n == 0 {
        return Fingerprint::default();
    }
    let nf = f64::from(u32::try_from(n).unwrap_or(u32::MAX));
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    let mut first_seen: BTreeMap<&str, usize> = BTreeMap::new();
    for (idx, tok) in toks.iter().enumerate() {
        *counts.entry(tok.as_str()).or_insert(0) += 1;
        first_seen.entry(tok.as_str()).or_insert(idx);
    }
    let per1k = |c: u64| round_dp(1000.0 * f64::from(u32::try_from(c).unwrap_or(u32::MAX)) / nf, 2);

    let sents = sentences(&text);
    let slens: Vec<f64> = sents
        .iter()
        .map(|s| f64::from(u32::try_from(tokenize(s).len()).unwrap_or(u32::MAX)))
        .collect();
    let paras = paragraphs(&text);
    let plens: Vec<f64> = paras
        .iter()
        .map(|p| f64::from(u32::try_from(tokenize(p).len()).unwrap_or(u32::MAX)))
        .collect();

    let mut function_words_per1k = BTreeMap::new();
    for w in FUNCTION_WORDS {
        function_words_per1k.insert((*w).to_string(), per1k(counts.get(w).copied().unwrap_or(0)));
    }
    let count_char = |c: char| text.matches(c).count();
    let punct_per1k = PunctRates {
        em_dash: per1k(u64::try_from(count_char('\u{2014}') + text.matches(" -- ").count()).unwrap_or(0)),
        semicolon: per1k(u64::try_from(count_char(';')).unwrap_or(0)),
        colon: per1k(u64::try_from(count_char(':')).unwrap_or(0)),
        comma: per1k(u64::try_from(count_char(',')).unwrap_or(0)),
        paren_open: per1k(u64::try_from(count_char('(')).unwrap_or(0)),
        question: per1k(u64::try_from(count_char('?')).unwrap_or(0)),
        exclaim: per1k(u64::try_from(count_char('!')).unwrap_or(0)),
        hyphenated_compounds: per1k(u64::try_from(count_hyphenated(&text)).unwrap_or(0)),
    };

    let fw_set: std::collections::BTreeSet<&str> = FUNCTION_WORDS.iter().copied().collect();
    let mut content: Vec<(&str, u64)> = counts
        .iter()
        .filter(|(w, _)| !fw_set.contains(**w) && w.chars().count() > 2)
        .map(|(w, c)| (*w, *c))
        .collect();
    content.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| {
            first_seen
                .get(a.0)
                .copied()
                .unwrap_or(usize::MAX)
                .cmp(&first_seen.get(b.0).copied().unwrap_or(usize::MAX))
        })
    });
    content.truncate(25);

    let sum_list = |list: &[&str]| -> u64 {
        list.iter().map(|w| counts.get(w).copied().unwrap_or(0)).sum()
    };
    let three_plus = toks.iter().filter(|t| syllables(t) >= 3).count();

    Fingerprint {
        words: n,
        sentences: sents.len(),
        sent_len_mean: if slens.is_empty() { 0.0 } else { round_dp(mean(&slens), 2) },
        sent_len_median: if slens.is_empty() {
            0.0
        } else {
            round_dp(median(&slens), 2)
        },
        sent_len_stdev: if slens.len() > 1 {
            round_dp(pstdev(&slens), 2)
        } else {
            0.0
        },
        sent_pct_short_le10: if slens.is_empty() {
            0.0
        } else {
            round_dp(
                100.0 * f64::from(u32::try_from(slens.iter().filter(|x| **x <= 10.0).count()).unwrap_or(0))
                    / f64::from(u32::try_from(slens.len()).unwrap_or(1)),
                1,
            )
        },
        sent_pct_long_ge30: if slens.is_empty() {
            0.0
        } else {
            round_dp(
                100.0 * f64::from(u32::try_from(slens.iter().filter(|x| **x >= 30.0).count()).unwrap_or(0))
                    / f64::from(u32::try_from(slens.len()).unwrap_or(1)),
                1,
            )
        },
        para_len_mean: if plens.is_empty() { 0.0 } else { round_dp(mean(&plens), 1) },
        para_len_median: if plens.is_empty() {
            0.0
        } else {
            round_dp(median(&plens), 1)
        },
        function_words_per1k,
        punct_per1k,
        type_token_ratio: round_dp(
            f64::from(u32::try_from(counts.len()).unwrap_or(u32::MAX)) / nf,
            3,
        ),
        pct_words_3plus_syll: round_dp(
            100.0 * f64::from(u32::try_from(three_plus).unwrap_or(0)) / nf,
            1,
        ),
        hedges_per1k: per1k(sum_list(HEDGES)),
        assertives_per1k: per1k(sum_list(ASSERTIVES)),
        first_person_per1k: per1k(sum_list(&["i", "we", "our", "us"])),
        we_per1k: per1k(counts.get("we").copied().unwrap_or(0)),
        top_content_words: content
            .into_iter()
            .map(|(w, c)| (w.to_string(), c))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_apostrophe_rule() {
        // Validation: one internal apostrophe joins; a second splits.
        assert_eq!(tokenize("Don't stop believin' now"), vec!["don't", "stop", "believin", "now"]);
        // Adversarial: leading apostrophe is not part of a token.
        assert_eq!(tokenize("'tis the season"), vec!["tis", "the", "season"]);
    }

    #[test]
    fn hyphenated_matching_is_non_overlapping() {
        assert_eq!(count_hyphenated("a-b-c"), 1);
        assert_eq!(count_hyphenated("a-b-c-d"), 2);
        assert_eq!(count_hyphenated("well-known fact-checker"), 2);
        assert_eq!(count_hyphenated("no hyphens here"), 0);
    }

    #[test]
    fn empty_text_measures_zero() {
        assert_eq!(measure("12345 !!!").words, 0);
        assert_eq!(measure("").words, 0);
    }

    #[test]
    fn sentence_split_needs_boundary_and_capital() {
        let fp = measure("One two. three four five. Six seven.");
        assert_eq!(fp.sentences, 2);
    }
}
