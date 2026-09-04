//! The short-phrase-with-nothing-to-fix heuristic.
//!
//! No prototype measurement exists for this (`AC_36.md`'s "what this stage
//! has to settle"). Read literally, `docs/design.md`'s own sentence has two
//! conditions joined by "and" plus an implicit third: a short phrase
//! containing neither foreign terms nor names from the context skips this
//! stage. See `tasks/36/DESIGN_36.md`, section 4.

/// A named constant, not a configuration key — the same footing
/// `bias::transcript::TURN_CHARS` is on. Short enough that a false skip
/// costs a missed punctuation fix, not a mangled technical term in a take
/// long enough to actually need one (`DESIGN_36.md` §4).
const SKIP_WORD_LIMIT: usize = 8;

pub fn plain(transcript: &str, bias: &str, enabled: bool) -> bool {
    if !enabled {
        return false;
    }
    let words: Vec<&str> = transcript.split_whitespace().collect();
    if words.len() > SKIP_WORD_LIMIT {
        return false;
    }
    if has_latin_run(transcript) {
        return false;
    }
    !shares_a_word(transcript, bias)
}

fn has_latin_run(text: &str) -> bool {
    let mut run = 0;
    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            run += 1;
            if run >= 2 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn shares_a_word(transcript: &str, bias: &str) -> bool {
    let bias_words: std::collections::HashSet<String> =
        bias.split_whitespace().map(|w| w.to_lowercase()).collect();
    transcript
        .split_whitespace()
        .any(|w| bias_words.contains(&w.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_never_skips() {
        assert!(!plain("да", "", false));
    }

    #[test]
    fn a_short_plain_phrase_with_no_context_name_skips() {
        assert!(plain("открой файл", "", true));
    }

    #[test]
    fn a_long_phrase_does_not_skip_even_with_no_foreign_term() {
        let long = "слово ".repeat(20);
        assert!(!plain(&long, "", true));
    }

    #[test]
    fn a_short_phrase_with_a_latin_run_does_not_skip() {
        assert!(!plain("открой pull request", "", true));
    }

    #[test]
    fn a_short_phrase_matching_a_context_word_does_not_skip() {
        // "журнал" (journal/log) has no Latin letters, so this transcript
        // passes both earlier checks — length and has_latin_run — and must
        // be caught by shares_a_word alone. A shares_a_word that always
        // returns false would pass every other test in this module but
        // fail this one and the next: neither transcript here contains any
        // ASCII letter, so has_latin_run cannot short-circuit before
        // shares_a_word runs, unlike a Latin loanword would.
        assert!(!plain("открой журнал", "журнал notes.txt", true));
    }

    #[test]
    fn the_context_match_is_case_insensitive() {
        assert!(!plain("открой ЖУРНАЛ", "журнал notes.txt", true));
    }
}
