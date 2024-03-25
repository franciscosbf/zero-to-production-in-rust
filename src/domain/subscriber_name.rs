use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, thiserror::Error)]
pub enum SubscriberNameError {
    #[error("Name is empty")]
    Empty,
    #[error("Name is too long")]
    TooLong,
    #[error("Name contains invalid characters")]
    InvalidCharacters,
}

#[derive(Debug)]
pub struct SubscriberName(String);

const FORBIDDEN_CHARS: &[char] = &['/', '(', ')', '"', '<', '>', '\\', '{', '}'];

impl SubscriberName {
    pub fn parse(s: String) -> Result<SubscriberName, SubscriberNameError> {
        let is_empty_or_whitespace = s.trim().is_empty();
        if is_empty_or_whitespace {
            return Err(SubscriberNameError::Empty);
        }

        // s must be less than 256 grapheme clusters.
        let is_too_long = s.graphemes(true).nth(256).is_some();
        if is_too_long {
            return Err(SubscriberNameError::TooLong);
        }

        let contains_forbidden_chars = s.chars().any(|g| FORBIDDEN_CHARS.contains(&g));
        if contains_forbidden_chars {
            return Err(SubscriberNameError::InvalidCharacters)?;
        }

        Ok(Self(s))
    }
}

impl AsRef<str> for SubscriberName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use claims::{assert_err, assert_ok};

    use super::{SubscriberName, FORBIDDEN_CHARS};

    #[test]
    fn a_256_graphemes_long_name_is_valid() {
        let name = "ë".repeat(256);
        assert_ok!(SubscriberName::parse(name));
    }

    #[test]
    fn a_name_longer_than_256_graphemes_is_rejected() {
        let name = "a".repeat(257);
        assert_err!(SubscriberName::parse(name));
    }

    #[test]
    fn whitespace_only_names_are_rejected() {
        let name = " ".to_string();
        assert_err!(SubscriberName::parse(name));
    }

    #[test]
    fn empty_string_is_rejected() {
        let name = "".to_string();
        assert_err!(SubscriberName::parse(name));
    }

    #[test]
    fn names_containing_invalid_char_are_rejected() {
        for name in FORBIDDEN_CHARS {
            let name = name.to_string();
            assert_err!(SubscriberName::parse(name));
        }
    }

    #[test]
    fn a_valid_name_is_parsed_successfully() {
        let name = "Francisco".to_string();
        assert_ok!(SubscriberName::parse(name));
    }
}
