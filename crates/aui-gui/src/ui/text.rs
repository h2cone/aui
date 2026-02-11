pub fn ellipsize(value: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipsize_noop_when_short() {
        assert_eq!(ellipsize("abc", 3), "abc");
        assert_eq!(ellipsize("abc", 10), "abc");
    }

    #[test]
    fn ellipsize_truncates_with_ellipsis() {
        assert_eq!(ellipsize("abcd", 3), "ab…");
        assert_eq!(ellipsize("hello world", 6), "hello…");
    }

    #[test]
    fn ellipsize_handles_zero_max() {
        assert_eq!(ellipsize("abc", 0), "");
    }
}
