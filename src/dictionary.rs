use std::collections::HashSet;
use std::io;
use std::sync::OnceLock;

use crate::layout::switch_layout_to;
use crate::types::Language;

fn debug_enabled() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| std::env::var_os("TYPELAN_DEBUG").is_some())
}

/// Load a plain-text word list (one word per line) into a `HashSet`.
///
/// For each entry, also inserts a punctuation-stripped variant (apostrophe and
/// double-quote removed) so that words like `don't` match a typed `dont` —
/// the English keymap can't produce `'`, so the original entries would
/// otherwise be unreachable.
pub fn load_dictionary(path: &str) -> io::Result<HashSet<String>> {
    let content = std::fs::read_to_string(path)?;
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
    Ok(dict)
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

/// Given the same physical key sequence interpreted as English (`word_en`) and
/// Hebrew (`word_he`), decide whether the layout must be switched and do it.
/// Returns `true` when a switch was performed.
pub fn check_and_switch_candidates(
    word_en: &str,
    word_he: &str,
    en_dict: &HashSet<String>,
    he_dict: &HashSet<String>,
) -> bool {
    // Keymap output is already lowercase (English) or caseless (Hebrew),
    // so no per-call lowercasing/allocation is needed.
    let is_in_en = !word_en.is_empty() && en_dict.contains(word_en);
    let is_in_he = !word_he.is_empty() && matches_hebrew(word_he, he_dict);

    let final_en = is_in_en && !is_in_he;
    let final_he = is_in_he && !is_in_en;

    let target_lang = if final_en {
        Some(Language::English)
    } else if final_he {
        Some(Language::Hebrew)
    } else {
        None
    };

    if debug_enabled() {
        println!("{}", word_en);
        println!("{}", word_he);
        println!("English: {}", if final_en { "True" } else { "False" });
        println!("Hebrew: {}", if final_he { "True" } else { "False" });
    }

    if let Some(lang) = target_lang {
        let switched = switch_layout_to(lang);
        if debug_enabled() {
            println!("Switch: {}", if switched { "True" } else { "False" });
        }
        switched
    } else {
        if debug_enabled() {
            println!("Switch: False");
        }
        false
    }
}
