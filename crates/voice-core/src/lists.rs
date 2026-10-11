//! Word lists transcribed verbatim from `scripts/stylometry.py` in the
//! matt-voice skill (the parity reference). Order is significant: the
//! function-word order breaks ties when the pooled top-30 is selected,
//! exactly as Python's stable sort over the source dict does.

/// Function words (deduplicated, first-occurrence order).
pub const FUNCTION_WORDS: &[&str] = &[
    "the", "of", "and", "a", "an", "in", "to", "is",
    "are", "was", "were", "be", "been", "being", "for", "with",
    "as", "by", "on", "at", "from", "or", "but", "if",
    "then", "than", "so", "such", "into", "through", "during", "before",
    "after", "above", "below", "between", "under", "again", "further", "once",
    "here", "there", "when", "where", "why", "how", "all", "any",
    "both", "each", "few", "more", "most", "other", "some", "no",
    "nor", "not", "only", "own", "same", "too", "very", "can",
    "will", "just", "should", "now", "i", "we", "you", "he",
    "she", "it", "they", "them", "his", "her", "its", "our",
    "their", "this", "that", "these", "those", "am", "have", "has",
    "had", "having", "do", "does", "did", "doing", "would", "could",
    "shall", "may", "might", "must", "per", "via", "among", "within",
    "without", "upon", "toward", "towards", "against", "along", "across", "behind",
    "beyond", "plus", "except", "about", "off", "over", "out", "up",
    "down",
];

/// Hedge words counted for `hedges_per1k`.
pub const HEDGES: &[&str] = &[
    "may", "might", "could", "perhaps", "possibly", "likely", "potentially", "suggests",
    "suggest", "suggested", "appears", "appear", "seems", "seem", "tends", "tend",
    "arguably", "relatively", "somewhat", "frequently", "often", "generally", "typically",
];

/// Assertive words counted for `assertives_per1k`.
pub const ASSERTIVES: &[&str] = &[
    "must", "necessitates", "necessitate", "ensures", "ensure", "demonstrates", "demonstrate", "clearly",
    "ultimately", "requires", "require", "demands", "demand", "establishes", "establish", "indeed",
];
