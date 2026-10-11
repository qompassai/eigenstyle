//! The deterministic draft checker — a port of `check_voice.py`
//! (Burrows-style mean |z| over the fingerprint's features, bands from
//! the fingerprint document), extended with the verify gate adapted
//! from HumanifyMe:
//!
//! - **Integrity vs source**: when the caller supplies the source text
//!   a draft was rewritten from, every number and URL in the source is
//!   accounted for in the draft; dropped numbers and dropped/broken
//!   URLs are reported, never silently passed.
//! - **Avoid-list enforcement** (HumanifyMe/Arcitext): phrases from the
//!   profile's negative checklist found in the draft are surfaced.
//!
//! The stylometric verdict stays purely deterministic: no LLM calls,
//! and a sample's or draft's instructional content is never followed.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{Result, VoiceError};
use crate::fingerprint::{FingerprintDoc, delta_against, feature_vector};
use crate::stylometry::measure;
use crate::{DRAFT_MIN_WORDS, DRAFT_TEXT_MAX_BYTES, TOP_FEATURES_REPORTED};

/// Verdict bands, mirroring the Python checker's strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    /// delta <= close_max.
    Close,
    /// close_max < delta <= drifting_max.
    Drifting,
    /// delta > drifting_max.
    OffVoice,
    /// Draft below the word floor; stylometry cannot judge it.
    Unreliable,
}

impl Verdict {
    /// Display string (matches the Python checker's verdict words).
    pub fn as_str(&self) -> &'static str {
        match self {
            Verdict::Close => "CLOSE",
            Verdict::Drifting => "DRIFTING",
            Verdict::OffVoice => "OFF-VOICE",
            Verdict::Unreliable => "UNRELIABLE",
        }
    }
}

/// One feature's contribution to the delta.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeatureDeviation {
    /// Feature name (e.g. `punct:em_dash`, `fw:the`).
    pub feature: String,
    /// Draft's measured value.
    pub draft: f64,
    /// Fingerprint reference mean (effective, corrections applied).
    pub reference_mean: f64,
    /// |z| contribution.
    pub z: f64,
}

/// Number/URL integrity findings.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IntegrityReport {
    /// Numbers present in the source but missing (or fewer) in the
    /// draft. Empty when no source was supplied.
    pub dropped_numbers: Vec<String>,
    /// URLs present in the source but missing (or fewer) in the draft.
    pub dropped_urls: Vec<String>,
    /// URLs in the draft that are malformed (empty host, or a host
    /// with no dot that is not localhost) — the broken-URL check.
    pub malformed_urls: Vec<String>,
}

/// One avoid-list phrase found in the draft.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AvoidHit {
    /// The phrase as listed.
    pub phrase: String,
    /// Occurrences in the draft (case-insensitive).
    pub count: usize,
}

/// The full check report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckReport {
    /// Draft token count.
    pub words: usize,
    /// Mean |z| delta; `None` when the verdict is UNRELIABLE.
    pub delta: Option<f64>,
    /// The verdict.
    pub verdict: Verdict,
    /// Bands the verdict was judged against.
    pub close_max: f64,
    /// Upper DRIFTING bound.
    pub drifting_max: f64,
    /// Top contributing features, |z| descending (ties: feature name
    /// descending, mirroring the Python tuple sort).
    pub top_features: Vec<FeatureDeviation>,
    /// Number/URL integrity findings.
    pub integrity: IntegrityReport,
    /// Avoid-list hits.
    pub avoid_hits: Vec<AvoidHit>,
}

/// Extract number runs: digits with internal `.`/`,` only when followed
/// by a digit, optional trailing `%`. Returns surface forms in order.
pub fn extract_numbers(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            i += 1;
            while i < chars.len() {
                let digit = chars[i].is_ascii_digit();
                let inner_punct = (chars[i] == '.' || chars[i] == ',')
                    && i + 1 < chars.len()
                    && chars[i + 1].is_ascii_digit();
                if digit || inner_punct {
                    i += 1;
                } else {
                    break;
                }
            }
            if i < chars.len() && chars[i] == '%' {
                i += 1;
            }
            out.push(chars[start..i].iter().collect());
        } else {
            i += 1;
        }
    }
    out
}

/// Extract URL-like substrings (`http://`, `https://`, `www.`),
/// trailing sentence punctuation stripped.
pub fn extract_urls(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let start = token
            .find("http://")
            .or_else(|| token.find("https://"))
            .or_else(|| token.find("www."));
        if let Some(idx) = start {
            let raw = &token[idx..];
            let trimmed = raw.trim_end_matches(['.', ',', ';', ')', ']', '}', '"', '\'']);
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
    }
    out
}

fn is_malformed_url(url: &str) -> bool {
    if let Some(rest) = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
    {
        let host = rest.split('/').next().unwrap_or("");
        host.is_empty() || (!host.contains('.') && host != "localhost")
    } else {
        false
    }
}

fn multiset_missing(source: &[String], draft: &[String]) -> Vec<String> {
    let mut available: BTreeMap<&str, usize> = BTreeMap::new();
    for item in draft {
        *available.entry(item.as_str()).or_insert(0) += 1;
    }
    let mut missing = Vec::new();
    for item in source {
        match available.get_mut(item.as_str()) {
            Some(n) if *n > 0 => *n -= 1,
            _ => missing.push(item.clone()),
        }
    }
    missing
}

/// Score a draft against the fingerprint.
///
/// - `draft` over [`DRAFT_TEXT_MAX_BYTES`] is rejected.
/// - `draft` under [`DRAFT_MIN_WORDS`] words yields verdict UNRELIABLE
///   with `delta = None` (integrity and avoid checks still run — they
///   do not depend on length).
/// - `source`, when supplied, enables dropped-number/URL accounting.
pub fn check_draft(
    doc: &FingerprintDoc,
    avoid_list: &[String],
    draft: &str,
    source: Option<&str>,
) -> Result<CheckReport> {
    if draft.len() > DRAFT_TEXT_MAX_BYTES {
        return Err(VoiceError::invalid(
            "text",
            format!("draft must be <= {DRAFT_TEXT_MAX_BYTES} bytes"),
        ));
    }
    let fp = measure(draft);
    let draft_urls = extract_urls(draft);
    let integrity = IntegrityReport {
        dropped_numbers: source
            .map(|s| multiset_missing(&extract_numbers(s), &extract_numbers(draft)))
            .unwrap_or_default(),
        dropped_urls: source
            .map(|s| multiset_missing(&extract_urls(s), &draft_urls))
            .unwrap_or_default(),
        malformed_urls: draft_urls
            .iter()
            .filter(|u| is_malformed_url(u))
            .cloned()
            .collect(),
    };
    let lowered = draft.to_lowercase();
    let avoid_hits: Vec<AvoidHit> = avoid_list
        .iter()
        .filter_map(|phrase| {
            let count = lowered.matches(phrase.to_lowercase().as_str()).count();
            (count > 0).then(|| AvoidHit {
                phrase: phrase.clone(),
                count,
            })
        })
        .collect();

    if fp.words < DRAFT_MIN_WORDS {
        return Ok(CheckReport {
            words: fp.words,
            delta: None,
            verdict: Verdict::Unreliable,
            close_max: doc.bands.close_max,
            drifting_max: doc.bands.drifting_max,
            top_features: Vec::new(),
            integrity,
            avoid_hits,
        });
    }
    let reference = doc.effective_reference();
    let vec = feature_vector(&fp);
    let delta = delta_against(&vec, &reference, &doc.features);
    let verdict = if delta <= doc.bands.close_max {
        Verdict::Close
    } else if delta <= doc.bands.drifting_max {
        Verdict::Drifting
    } else {
        Verdict::OffVoice
    };
    let mut deviations: Vec<FeatureDeviation> = doc
        .features
        .iter()
        .map(|f| {
            let draft_value = vec.get(f).copied().unwrap_or(0.0);
            let stat = reference
                .get(f)
                .copied()
                .unwrap_or(crate::fingerprint::RefStat { mean: 0.0, sd: 1.0 });
            FeatureDeviation {
                feature: f.clone(),
                draft: draft_value,
                reference_mean: stat.mean,
                z: (draft_value - stat.mean).abs() / stat.sd,
            }
        })
        .collect();
    deviations.sort_by(|a, b| {
        b.z.partial_cmp(&a.z)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.feature.cmp(&a.feature))
    });
    deviations.truncate(TOP_FEATURES_REPORTED);
    Ok(CheckReport {
        words: fp.words,
        delta: Some(delta),
        verdict,
        close_max: doc.bands.close_max,
        drifting_max: doc.bands.drifting_max,
        top_features: deviations,
        integrity,
        avoid_hits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FingerprintDoc;
    use crate::corpus::{Authorship, Corpus, NewSample};

    fn doc() -> FingerprintDoc {
        let mut corpus = Corpus::new();
        for (title, text) in [
            (
                "A",
                "The committee reviewed the evidence in full. Every claim was checked against the record, and the record was plain. We therefore conclude that the intervention is safe, that it is effective, and that it should be offered to patients who need it most. Whether in the operating room or in the clinic, the obligation is the same. Deliver care that is careful, measured, and honest about what the evidence shows.",
            ),
            (
                "B",
                "Current projections estimate 930,000 primary procedures and approximately 202,966 revisions for a total of 1,132,966 surgeries by 2030. The numbers are not small. They demand a response that is proportionate, careful, and grounded in outcomes that patients themselves report. A score that patients understand may help. It may also mislead, if the validation is weak.",
            ),
        ] {
            corpus
                .add_sample(NewSample {
                    title: title.to_string(),
                    genre: "scholarly".to_string(),
                    authorship: Authorship::Sole,
                    source: "test".to_string(),
                    text: text.repeat(6),
                })
                .expect("valid");
        }
        FingerprintDoc::build(&corpus).expect("builds")
    }

    #[test]
    fn short_draft_is_unreliable_not_scored() {
        let report = check_draft(&doc(), &[], "Short text here.", None).expect("checks");
        assert_eq!(report.verdict, Verdict::Unreliable);
        assert!(report.delta.is_none());
    }

    #[test]
    fn integrity_catches_dropped_numbers_and_urls() {
        let source = "In 2030 we project 1,132,966 surgeries (see https://example.org/report). Costs rose 12%.";
        let draft =
            "In the future we project many surgeries (see https://example.org/report). Costs rose.";
        let report = check_draft(&doc(), &[], draft, Some(source)).expect("checks");
        assert_eq!(
            report.integrity.dropped_numbers,
            vec!["2030", "1,132,966", "12%"]
        );
        assert!(report.integrity.dropped_urls.is_empty());
        // Adversarial: a dropped URL is caught.
        let draft2 = "In 2030 we project 1,132,966 surgeries. Costs rose 12%.";
        let report2 = check_draft(&doc(), &[], draft2, Some(source)).expect("checks");
        assert_eq!(
            report2.integrity.dropped_urls,
            vec!["https://example.org/report"]
        );
    }

    #[test]
    fn malformed_urls_flagged_and_avoid_list_enforced() {
        let draft = "Furthermore, see http:// for the pivotal landscape overview.";
        let avoid = vec![
            "furthermore".to_string(),
            "pivotal".to_string(),
            "landscape".to_string(),
        ];
        let report = check_draft(&doc(), &avoid, draft, None).expect("checks");
        assert_eq!(report.integrity.malformed_urls, vec!["http://"]);
        assert_eq!(report.avoid_hits.len(), 3);
    }

    #[test]
    fn oversized_draft_rejected() {
        let big = "word ".repeat(300_000);
        assert!(check_draft(&doc(), &[], &big, None).is_err());
    }
}
