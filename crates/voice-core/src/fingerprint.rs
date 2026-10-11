//! The fingerprint document: per-feature reference mean/SD across the
//! corpus pieces plus verdict bands calibrated by leave-one-out (LOO)
//! deltas among the pieces themselves.
//!
//! This replicates `build_fingerprint.py` from the voice-profile build
//! exactly: same feature set (15 base features + pooled top-30 function
//! words), same SD floors, same rounding (3 dp), same band rule
//! (close = max LOO delta, drifting = 1.5x). Bands are derived from
//! corpus pieces only — never tuned against drafts being judged.
//!
//! On top of the Python shape, the document carries user `corrections`:
//! values Matt has overridden through the UIs (HumanifyMe's editable
//! fingerprint). Corrections overlay the computed reference at read
//! time and survive recompute; the computed values are never mutated,
//! so a reset restores them exactly.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::corpus::Corpus;
use crate::error::{Result, VoiceError};
use crate::lists::FUNCTION_WORDS;
use crate::stylometry::{Fingerprint, measure, round_dp};
use crate::{FINGERPRINT_MIN_PIECES, TOP_FUNCTION_WORD_COUNT};

/// Base (non-function-word) features, in the build script's order.
pub const BASE_FEATURES: [&str; 15] = [
    "punct:em_dash",
    "punct:semicolon",
    "punct:colon",
    "punct:comma",
    "punct:paren_open",
    "punct:question",
    "punct:hyphenated_compounds",
    "sent_len_mean",
    "sent_len_median",
    "sent_pct_short_le10",
    "sent_pct_long_ge30",
    "hedges_per1k",
    "assertives_per1k",
    "first_person_per1k",
    "pct_words_3plus_syll",
];

/// Reference statistic for one feature.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct RefStat {
    /// Mean across corpus pieces.
    pub mean: f64,
    /// Population SD across pieces, floored (see `floor_sd`).
    pub sd: f64,
}

/// Verdict bands on the checker's mean-|z| delta.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Bands {
    /// delta <= close_max → CLOSE.
    pub close_max: f64,
    /// delta <= drifting_max → DRIFTING; above → OFF-VOICE.
    pub drifting_max: f64,
}

/// How the bands were calibrated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Calibration {
    /// Human-readable method note.
    pub method: String,
    /// Leave-one-out delta per corpus piece (keyed by piece title).
    pub loo_deltas: BTreeMap<String, f64>,
}

/// A user correction to one feature's reference values. Presence in the
/// corrections map marks the value as user-corrected.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Correction {
    /// Corrected mean, if the user set one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean: Option<f64>,
    /// Corrected SD, if the user set one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sd: Option<f64>,
}

/// The fingerprint document (seed shape = the skill's fingerprint.json,
/// plus the corrections overlay).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FingerprintDoc {
    /// Feature names in checker order (45 for the seed corpus).
    pub features: Vec<String>,
    /// Computed reference per feature.
    pub reference: BTreeMap<String, RefStat>,
    /// Verdict bands.
    pub bands: Bands,
    /// Band calibration record.
    pub calibration: Calibration,
    /// Corpus pieces the reference was built from (titles; the seed
    /// document records the original file names).
    pub corpus_pieces: Vec<String>,
    /// User corrections overlay (empty until Matt corrects a value).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub corrections: BTreeMap<String, Correction>,
}

/// Per-feature change between two fingerprint documents.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FeatureDelta {
    /// Feature name.
    pub feature: String,
    /// Reference mean before.
    pub old_mean: f64,
    /// Reference mean after.
    pub new_mean: f64,
    /// Reference SD before.
    pub old_sd: f64,
    /// Reference SD after.
    pub new_sd: f64,
}

/// The full 45+-feature vector for one measured fingerprint, keyed by
/// the checker's feature names (mirrors `check_voice.feature_vector`).
pub fn feature_vector(fp: &Fingerprint) -> BTreeMap<String, f64> {
    let mut vec = BTreeMap::new();
    let p = &fp.punct_per1k;
    vec.insert("punct:em_dash".to_string(), p.em_dash);
    vec.insert("punct:semicolon".to_string(), p.semicolon);
    vec.insert("punct:colon".to_string(), p.colon);
    vec.insert("punct:comma".to_string(), p.comma);
    vec.insert("punct:paren_open".to_string(), p.paren_open);
    vec.insert("punct:question".to_string(), p.question);
    vec.insert(
        "punct:hyphenated_compounds".to_string(),
        p.hyphenated_compounds,
    );
    vec.insert("sent_len_mean".to_string(), fp.sent_len_mean);
    vec.insert("sent_len_median".to_string(), fp.sent_len_median);
    vec.insert("sent_pct_short_le10".to_string(), fp.sent_pct_short_le10);
    vec.insert("sent_pct_long_ge30".to_string(), fp.sent_pct_long_ge30);
    vec.insert("hedges_per1k".to_string(), fp.hedges_per1k);
    vec.insert("assertives_per1k".to_string(), fp.assertives_per1k);
    vec.insert("first_person_per1k".to_string(), fp.first_person_per1k);
    vec.insert("pct_words_3plus_syll".to_string(), fp.pct_words_3plus_syll);
    for (w, v) in &fp.function_words_per1k {
        vec.insert(format!("fw:{w}"), *v);
    }
    vec
}

/// SD floor from the build script: rate-like features get
/// max(sd, 0.5, 0.2*|mean|); everything else gets max(sd, 2.0).
fn floor_sd(feature: &str, mean: f64, sd: f64) -> f64 {
    if feature.starts_with("fw:")
        || feature.starts_with("punct:")
        || feature.starts_with("hedges")
        || feature.starts_with("assertives")
        || feature.starts_with("first_person")
    {
        sd.max(0.5).max(0.2 * mean.abs())
    } else {
        sd.max(2.0)
    }
}

fn stats(vecs: &[BTreeMap<String, f64>], features: &[String]) -> BTreeMap<String, RefStat> {
    let mut out = BTreeMap::new();
    for f in features {
        let vals: Vec<f64> = vecs.iter().filter_map(|v| v.get(f).copied()).collect();
        let mean = if vals.is_empty() {
            0.0
        } else {
            vals.iter().sum::<f64>() / f64::from(u32::try_from(vals.len()).unwrap_or(1))
        };
        let sd = if vals.len() > 1 {
            let var = vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>()
                / f64::from(u32::try_from(vals.len()).unwrap_or(1));
            var.sqrt()
        } else {
            0.0
        };
        out.insert(
            f.clone(),
            RefStat {
                mean: round_dp(mean, 3),
                sd: round_dp(floor_sd(f, mean, sd), 3),
            },
        );
    }
    out
}

/// Mean |z| of a feature vector against a reference (the checker's
/// delta; also the LOO calibration distance).
pub fn delta_against(
    vec: &BTreeMap<String, f64>,
    reference: &BTreeMap<String, RefStat>,
    features: &[String],
) -> f64 {
    if features.is_empty() {
        return 0.0;
    }
    let sum: f64 = features
        .iter()
        .map(|f| {
            let v = vec.get(f).copied().unwrap_or(0.0);
            let r = reference
                .get(f)
                .copied()
                .unwrap_or(RefStat { mean: 0.0, sd: 1.0 });
            (v - r.mean).abs() / r.sd
        })
        .sum();
    sum / f64::from(u32::try_from(features.len()).unwrap_or(1))
}

impl FingerprintDoc {
    /// Build a fingerprint from the corpus's included samples,
    /// replicating `build_fingerprint.py`. Existing corrections are
    /// NOT carried over — the caller (store) reapplies them so the
    /// overlay is explicit.
    ///
    /// Fails closed below [`FINGERPRINT_MIN_PIECES`] included pieces:
    /// LOO calibration is undefined there.
    pub fn build(corpus: &Corpus) -> Result<FingerprintDoc> {
        let pieces = corpus.included_samples();
        if pieces.len() < FINGERPRINT_MIN_PIECES {
            return Err(VoiceError::WrongState {
                message: format!(
                    "fingerprint needs >= {FINGERPRINT_MIN_PIECES} included pieces, has {}",
                    pieces.len()
                ),
            });
        }
        let vecs: Vec<BTreeMap<String, f64>> = pieces
            .iter()
            .map(|s| feature_vector(&measure(&s.text)))
            .collect();
        let pooled_text: String = pieces.iter().map(|s| s.text.as_str()).collect();
        let pooled = measure(&pooled_text);
        let mut fw_ranked: Vec<(&String, &f64)> = pooled.function_words_per1k.iter().collect();
        fw_ranked.sort_by(|a, b| {
            b.1.partial_cmp(a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let ia = FUNCTION_WORDS
                        .iter()
                        .position(|w| *w == a.0.as_str())
                        .unwrap_or(usize::MAX);
                    let ib = FUNCTION_WORDS
                        .iter()
                        .position(|w| *w == b.0.as_str())
                        .unwrap_or(usize::MAX);
                    ia.cmp(&ib)
                })
        });
        let top_fw: Vec<String> = fw_ranked
            .iter()
            .take(TOP_FUNCTION_WORD_COUNT)
            .map(|(w, _)| format!("fw:{w}"))
            .collect();
        let mut features: Vec<String> = BASE_FEATURES.iter().map(ToString::to_string).collect();
        features.extend(top_fw);

        let reference = stats(&vecs, &features);
        let mut loo_deltas = BTreeMap::new();
        for (i, piece) in pieces.iter().enumerate() {
            let others: Vec<BTreeMap<String, f64>> = vecs
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, v)| v.clone())
                .collect();
            let loo_ref = stats(&others, &features);
            let d = delta_against(&vecs[i], &loo_ref, &features);
            loo_deltas.insert(piece.title.clone(), round_dp(d, 3));
        }
        let max_loo = loo_deltas.values().copied().fold(0.0f64, f64::max);
        let bands = Bands {
            close_max: round_dp(max_loo, 3),
            drifting_max: round_dp(1.5 * max_loo, 3),
        };
        Ok(FingerprintDoc {
            features,
            reference,
            bands,
            calibration: Calibration {
                method: format!(
                    "leave-one-out deltas among the {} corpus pieces; close_max = max LOO delta; drifting_max = 1.5x max LOO",
                    pieces.len()
                ),
                loo_deltas,
            },
            corpus_pieces: pieces.iter().map(|s| s.title.clone()).collect(),
            corrections: BTreeMap::new(),
        })
    }

    /// The reference with user corrections overlaid. This is what the
    /// checker scores against.
    pub fn effective_reference(&self) -> BTreeMap<String, RefStat> {
        let mut out = self.reference.clone();
        for (feature, correction) in &self.corrections {
            if let Some(stat) = out.get_mut(feature) {
                if let Some(mean) = correction.mean {
                    stat.mean = mean;
                }
                if let Some(sd) = correction.sd {
                    stat.sd = sd;
                }
            }
        }
        out
    }

    /// Whether a feature's effective value comes from a user correction.
    pub fn is_corrected(&self, feature: &str) -> bool {
        self.corrections.contains_key(feature)
    }

    /// Set (or partially update) a user correction. Validated: the
    /// feature must exist, values finite, SD strictly positive (a zero
    /// SD would make the checker's z-scores meaningless).
    pub fn set_correction(
        &mut self,
        feature: &str,
        mean: Option<f64>,
        sd: Option<f64>,
    ) -> Result<()> {
        if !self.reference.contains_key(feature) {
            return Err(VoiceError::NotFound {
                what: format!("feature {feature}"),
            });
        }
        if mean.is_some_and(|m| !m.is_finite()) {
            return Err(VoiceError::invalid("mean", "must be finite"));
        }
        if sd.is_some_and(|s| !s.is_finite() || s <= 0.0) {
            return Err(VoiceError::invalid("sd", "must be finite and > 0"));
        }
        let entry = self.corrections.entry(feature.to_string()).or_default();
        if mean.is_some() {
            entry.mean = mean;
        }
        if sd.is_some() {
            entry.sd = sd;
        }
        Ok(())
    }

    /// Drop all corrections, restoring computed values exactly.
    pub fn clear_corrections(&mut self) {
        self.corrections.clear();
    }

    /// Features whose computed reference changed vs `other`, for the
    /// recompute preview/report (the Eden show-before-save step).
    pub fn diff(&self, other: &FingerprintDoc) -> Vec<FeatureDelta> {
        let mut out = Vec::new();
        for f in &self.features {
            let (Some(a), Some(b)) = (self.reference.get(f), other.reference.get(f)) else {
                continue;
            };
            if (a.mean - b.mean).abs() > f64::EPSILON || (a.sd - b.sd).abs() > f64::EPSILON {
                out.push(FeatureDelta {
                    feature: f.clone(),
                    old_mean: b.mean,
                    new_mean: a.mean,
                    old_sd: b.sd,
                    new_sd: a.sd,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{Authorship, NewSample};

    fn corpus_with(pieces: &[&str]) -> Corpus {
        let mut corpus = Corpus::new();
        for (i, text) in pieces.iter().enumerate() {
            corpus
                .add_sample(NewSample {
                    title: format!("Piece {}", i + 1),
                    genre: "scholarly".to_string(),
                    authorship: Authorship::Sole,
                    source: "test".to_string(),
                    text: text.to_string(),
                })
                .expect("valid piece");
        }
        corpus
    }

    const PIECE_A: &str = "The committee reviewed the evidence in full. Every claim was checked against the record, and the record was plain. We therefore conclude that the intervention is safe, that it is effective, and that it should be offered to patients who need it most. Whether in the operating room or in the clinic, the obligation is the same. Deliver care that is careful, measured, and honest about what the evidence shows.";
    const PIECE_B: &str = "Current projections estimate 930,000 primary procedures and approximately 202,966 revisions for a total of 1,132,966 surgeries by 2030. The numbers are not small. They demand a response that is proportionate, careful, and grounded in outcomes that patients themselves report. A score that patients understand may help. It may also mislead, if the validation is weak.";

    #[test]
    fn build_produces_bands_and_features() {
        let corpus = corpus_with(&[PIECE_A, PIECE_B]);
        let doc = FingerprintDoc::build(&corpus).expect("two pieces suffice");
        assert_eq!(doc.features.len(), 45);
        assert!(doc.bands.close_max > 0.0);
        assert!(doc.bands.drifting_max > doc.bands.close_max);
        assert_eq!(doc.calibration.loo_deltas.len(), 2);
    }

    #[test]
    fn build_fails_closed_below_two_pieces() {
        let corpus = corpus_with(&[PIECE_A]);
        assert!(FingerprintDoc::build(&corpus).is_err());
    }

    #[test]
    fn corrections_overlay_and_reset() {
        let corpus = corpus_with(&[PIECE_A, PIECE_B]);
        let mut doc = FingerprintDoc::build(&corpus).expect("builds");
        let computed = doc.reference["sent_len_mean"];
        doc.set_correction("sent_len_mean", Some(99.0), None)
            .expect("known feature");
        assert!(doc.is_corrected("sent_len_mean"));
        assert_eq!(doc.effective_reference()["sent_len_mean"].mean, 99.0);
        assert_eq!(doc.reference["sent_len_mean"], computed);
        // Adversarial: zero SD and unknown features are refused.
        assert!(
            doc.set_correction("sent_len_mean", None, Some(0.0))
                .is_err()
        );
        assert!(
            doc.set_correction("fw:nonexistent", Some(1.0), None)
                .is_err()
        );
        doc.clear_corrections();
        assert!(!doc.is_corrected("sent_len_mean"));
    }
}
