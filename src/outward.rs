//! Running somebody else's program with a bound on how long it may take, and
//! cutting what it printed. `docs/decisions.md` holds the bound for each call.

/// `text` cut to at most `limit` bytes on a character boundary.
///
/// `String::truncate` panics when the length is not on a boundary, and program
/// output reaches here through `String::from_utf8_lossy`, where any multi-byte
/// character, or the replacement character, can straddle the limit (issue #94).
#[allow(dead_code)]
pub fn shorten(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let mut end = limit;
    // Zero is always a boundary, so this ends.
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_that_fits_is_returned_unchanged() {
        assert_eq!(shorten("short", 400), "short");
        assert_eq!(shorten("exact", 5), "exact");
    }

    #[test]
    fn ascii_is_cut_at_the_limit() {
        assert_eq!(shorten("abcdef", 3), "abc");
    }

    #[test]
    fn a_limit_of_zero_gives_nothing() {
        assert_eq!(shorten("abc", 0), "");
    }

    #[test]
    fn a_two_byte_character_across_the_limit_is_dropped_whole() {
        let text = format!("{}é", "a".repeat(399));
        assert_eq!(text.len(), 401);
        assert_eq!(shorten(&text, 400), "a".repeat(399));
    }

    #[test]
    fn a_three_byte_character_across_the_limit_is_dropped_whole() {
        let text = format!("{}—", "a".repeat(398));
        assert_eq!(text.len(), 401);
        assert_eq!(shorten(&text, 400), "a".repeat(398));
        assert_eq!(shorten(&text, 399), "a".repeat(398));
    }

    #[test]
    fn a_four_byte_character_across_the_limit_is_dropped_whole() {
        let text = format!("{}😀", "a".repeat(397));
        assert_eq!(text.len(), 401);
        assert_eq!(shorten(&text, 400), "a".repeat(397));
    }

    #[test]
    fn no_limit_panics_on_cyrillic_text() {
        let text = "ошибка чтения файла".repeat(30);
        for limit in 0..=text.len() + 1 {
            let cut = shorten(&text, limit);
            assert!(cut.len() <= limit, "limit {limit}: {} bytes", cut.len());
            assert!(text.starts_with(&cut), "limit {limit}");
        }
    }
}
