//! Whether a transcript is one phrase said over and over.
//!
//! A transcriber asked to decode silence tends to answer with a single phrase
//! repeated: a minute of room tone came back as "Продолжение следует..." four
//! times over (`docs/evidence.md`). The mean level of a take cannot tell that
//! take from quiet speech, so the guard reads the output instead. It needs no
//! threshold and no configuration, and it only reports: what to do with a
//! flagged transcript is the daemon's decision (`tasks/30/DESIGN_30.md`).

/// What was found: the block repeated, and how often.
#[derive(Debug, PartialEq, Eq)]
pub struct Repetition {
    /// How many times the block occurs; at least 3.
    pub times: usize,
    /// How many words the block has; at least 1.
    pub words: usize,
}

/// Whether the whole of `text` is one block of words repeated three times or
/// more. Case and the punctuation around each word are ignored, and a piece that
/// is only punctuation is not a word. Any other word, before, between or after
/// the copies, means no.
pub fn one_phrase_repeated(text: &str) -> Option<Repetition> {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|piece| {
            piece
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect();
    let n = words.len();
    // The smallest block first, so "a b a b a b a b" is four copies of two words
    // and not two copies of four.
    (1..=n / 3).find_map(|m| {
        (n % m == 0 && words.iter().enumerate().all(|(i, w)| *w == words[i % m])).then_some(
            Repetition {
                times: n / m,
                words: m,
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_example_from_the_issue_is_one_phrase_repeated_four_times() {
        assert_eq!(
            one_phrase_repeated(
                "Продолжение следует... Продолжение следует... Продолжение следует... \
                 Продолжение следует..."
            ),
            Some(Repetition { times: 4, words: 2 })
        );
    }

    #[test]
    fn case_and_punctuation_do_not_matter() {
        assert_eq!(
            one_phrase_repeated("Thank you. THANK YOU! thank, you"),
            Some(Repetition { times: 3, words: 2 })
        );
    }

    #[test]
    fn one_word_three_times_is_flagged() {
        assert_eq!(
            one_phrase_repeated("no no no"),
            Some(Repetition { times: 3, words: 1 })
        );
    }

    #[test]
    fn the_smallest_block_is_reported() {
        assert_eq!(
            one_phrase_repeated("a b a b a b a b"),
            Some(Repetition { times: 4, words: 2 })
        );
    }

    #[test]
    fn newlines_between_the_copies_do_not_matter() {
        assert_eq!(
            one_phrase_repeated("thank you\nthank you\nthank you"),
            Some(Repetition { times: 3, words: 2 })
        );
    }

    #[test]
    fn a_block_twice_is_not_flagged() {
        assert_eq!(one_phrase_repeated("thank you thank you"), None);
    }

    #[test]
    fn a_partial_block_after_the_copies_is_not_flagged() {
        // Seven words: three copies of "a b" and the start of a fourth. The
        // whole text is not one block repeated.
        assert_eq!(one_phrase_repeated("a b a b a b a"), None);
        assert_eq!(
            one_phrase_repeated("thank you thank you thank you thank"),
            None
        );
    }

    #[test]
    fn the_smallest_block_wins_when_two_sizes_fit() {
        // Twelve words fit blocks of 2, 4 and 6 words. The smallest is two
        // words six times, not four words three times.
        assert_eq!(
            one_phrase_repeated("a b a b a b a b a b a b"),
            Some(Repetition { times: 6, words: 2 })
        );
    }

    #[test]
    fn three_copies_with_another_word_are_not_flagged() {
        for text in [
            "so thank you thank you thank you",
            "thank you so thank you thank you",
            "thank you thank you thank you so",
            "thank you thank you thank",
        ] {
            assert_eq!(one_phrase_repeated(text), None, "{text:?}");
        }
    }

    #[test]
    fn ordinary_speech_that_repeats_a_word_is_not_flagged() {
        assert_eq!(one_phrase_repeated("I said no no and then no"), None);
        assert_eq!(
            one_phrase_repeated("it is what it is and that is what it is"),
            None
        );
    }

    #[test]
    fn three_different_words_are_not_flagged() {
        assert_eq!(one_phrase_repeated("one two three"), None);
    }

    #[test]
    fn empty_and_tiny_texts_are_not_flagged() {
        for text in ["", "   ", "...", "hello", "hello hello"] {
            assert_eq!(one_phrase_repeated(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_dash_on_its_own_is_not_a_word() {
        assert_eq!(
            one_phrase_repeated("yes — yes — yes"),
            Some(Repetition { times: 3, words: 1 })
        );
    }
}
