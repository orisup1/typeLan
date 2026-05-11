use std::collections::HashSet;
use std::sync::OnceLock;

use crate::layout::switch_layout_to;
use crate::types::Language;

fn debug_enabled() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| std::env::var_os("TYPELAN_DEBUG").is_some())
}

/// Parse a plain-text word list (one word per line) into a `HashSet`.
///
/// For each entry, also inserts a punctuation-stripped variant (apostrophe and
/// double-quote removed) so that words like `don't` match a typed `dont` —
/// the English keymap can't produce `'`, so the original entries would
/// otherwise be unreachable.
///
/// Operates on an already-loaded string (the dictionaries are embedded via
/// `include_str!` in `main.rs`, so the binary is self-contained and can be
/// run from any working directory).
pub fn parse_dictionary(content: &str) -> HashSet<String> {
    let mut dict = HashSet::with_capacity(content.len() / 8);
    for line in content.lines() {
        let word = line.trim();
        if word.is_empty() {
            continue;
        }
        // ASCII-only lowercase: faster than Unicode `to_lowercase`. Hebrew has
        // no case, English entries are ASCII, so byte-level folding suffices.
        let lower = word.to_ascii_lowercase();
        if lower.bytes().any(|b| b == b'\'' || b == b'"') {
            let stripped: String =
                lower.chars().filter(|c| *c != '\'' && *c != '"').collect();
            if !stripped.is_empty() {
                dict.insert(stripped);
            }
        }
        dict.insert(lower);
    }
    dict
}

/// One-letter inflectional prefixes that Hebrew attaches to nouns/verbs:
/// ו (and), ה (the), ל (to/for), ב (in), כ (as/like), מ (from), ש (that).
const HE_PREFIXES: &[char] = &['ו', 'ה', 'ל', 'ב', 'כ', 'מ', 'ש'];

/// Hebrew lookup with single-prefix fallback: if the word is not in the dict
/// directly, try stripping a leading prefix letter and looking up the rest.
/// Only one prefix is stripped to avoid over-matching; the dictionary already
/// holds many common prefixed forms as full entries.
fn matches_hebrew(word: &str, dict: &HashSet<String>) -> bool {
    if dict.contains(word) {
        return true;
    }
    let mut iter = word.chars();
    if let Some(first) = iter.next() {
        if HE_PREFIXES.contains(&first) {
            let rest = iter.as_str();
            if !rest.is_empty() && dict.contains(rest) {
                return true;
            }
        }
    }
    false
}

/// Pure decision: given the same physical key sequence interpreted as English
/// (`word_en`) and Hebrew (`word_he`), return the layout to switch to — or
/// `None` if the word is in both dicts (ambiguous) or in neither.
///
/// The Hebrew side uses a strict direct dict lookup here, NOT the looser
/// `matches_hebrew` (prefix-strip) helper. Reason: when the user types an
/// English word that is not in `en_dict` (a name, slang, plural, typo,
/// short abbreviation, etc.), the same key sequence interpreted as Hebrew
/// often pattern-matches "valid Hebrew word with one of the one-letter
/// inflectional prefixes ו ה ל ב כ מ ש". The prefix-strip rescue then fires
/// and we wrongly flip to Hebrew. Requiring a direct `he_dict` hit kills
/// that false positive at the cost of missing some real prefixed-Hebrew
/// corrections (the dict already carries many prefixed forms outright).
///
/// `matches_hebrew` is still used by `is_known_word` below — there the
/// looser match is gated by the suffix also having to decide for a layout,
/// so the false-positive rate is naturally lower.
fn decide_target_lang(
    word_en: &str,
    word_he: &str,
    en_dict: &HashSet<String>,
    he_dict: &HashSet<String>,
) -> Option<Language> {
    let is_in_en = !word_en.is_empty() && en_dict.contains(word_en);
    let is_in_he = !word_he.is_empty() && he_dict.contains(word_he);
    if is_in_en && !is_in_he {
        Some(Language::English)
    } else if is_in_he && !is_in_en {
        Some(Language::Hebrew)
    } else {
        None
    }
}

/// True if the key sequence is recognised as a word in either dictionary
/// (used as the prefix-validity check when looking for a missing-space split).
fn is_known_word(
    word_en: &str,
    word_he: &str,
    en_dict: &HashSet<String>,
    he_dict: &HashSet<String>,
) -> bool {
    (!word_en.is_empty() && en_dict.contains(word_en))
        || (!word_he.is_empty() && matches_hebrew(word_he, he_dict))
}

fn debug_log(word_en: &str, word_he: &str, target: Option<Language>, switched: bool) {
    if !debug_enabled() {
        return;
    }
    println!("{}", word_en);
    println!("{}", word_he);
    println!(
        "English: {}",
        if matches!(target, Some(Language::English)) { "True" } else { "False" }
    );
    println!(
        "Hebrew: {}",
        if matches!(target, Some(Language::Hebrew)) { "True" } else { "False" }
    );
    println!("Switch: {}", if switched { "True" } else { "False" });
}

/// Run the layout-switch decision over a key sequence.
///
/// First tries the full buffer (parity with the historical behaviour). If that
/// yields no decision, scans split points from longest prefix down: when the
/// prefix is itself a known word in some dictionary and the suffix decides for
/// a specific layout, treat the suffix as the current word — this catches the
/// case where the user forgot the space between two words (e.g. "hellohello").
///
/// Returns `Some(start)` when a switch was performed; the suffix that was
/// acted on begins at `keys[start]` (so `start = 0` means the whole buffer
/// was used). Callers should delete and retype only `keys[start..]`.
pub fn check_and_switch_with_split<K: Copy>(
    keys: &[K],
    to_en: impl Fn(K) -> Option<char>,
    to_he: impl Fn(K) -> Option<char>,
    en_dict: &HashSet<String>,
    he_dict: &HashSet<String>,
) -> Option<usize> {
    if keys.is_empty() {
        return None;
    }
    let build = |slice: &[K]| -> (String, String) {
        let en: String = slice.iter().filter_map(|&k| to_en(k)).collect();
        let he: String = slice.iter().filter_map(|&k| to_he(k)).collect();
        (en, he)
    };

    // 1. Full-buffer attempt — preserves the original behaviour exactly.
    let (full_en, full_he) = build(keys);
    if let Some(lang) = decide_target_lang(&full_en, &full_he, en_dict, he_dict) {
        let switched = switch_layout_to(lang);
        debug_log(&full_en, &full_he, Some(lang), switched);
        return if switched { Some(0) } else { None };
    }

    // 2. Split fallback. Scan split points from longest prefix to shortest;
    //    the first match leaves the most user-typed text intact.
    for split in (1..keys.len()).rev() {
        let (prefix_en, prefix_he) = build(&keys[..split]);
        if !is_known_word(&prefix_en, &prefix_he, en_dict, he_dict) {
            continue;
        }
        let (suffix_en, suffix_he) = build(&keys[split..]);
        let Some(lang) = decide_target_lang(&suffix_en, &suffix_he, en_dict, he_dict) else {
            continue;
        };
        let switched = switch_layout_to(lang);
        if debug_enabled() {
            println!(
                "split @ {}: prefix=[{} / {}] suffix=[{} / {}]",
                split, prefix_en, prefix_he, suffix_en, suffix_he,
            );
        }
        debug_log(&suffix_en, &suffix_he, Some(lang), switched);
        if switched {
            return Some(split);
        }
        // Layout was already correct for this suffix; keep scanning shorter
        // prefixes in case a different split decides differently.
    }

    debug_log(&full_en, &full_he, None, false);
    None
}
