#[derive(Debug, thiserror::Error)]
#[error("{0} is not a valid token")]
pub struct TokenError(String);

#[derive(Debug)]
pub struct Token(String);

impl Token {
    pub fn parse(s: String) -> Result<Token, TokenError> {
        let is_empty_or_whitespace = s.trim().is_empty();

        let has_invalid_size = s.len() != 30;

        let contains_forbidden_chars = s.chars().any(|c| !c.is_ascii_alphanumeric());

        if is_empty_or_whitespace || has_invalid_size || contains_forbidden_chars {
            Err(TokenError(s))
        } else {
            Ok(Self(s))
        }
    }
}

impl AsRef<str> for Token {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use claims::{assert_err, assert_ok};

    use super::Token;

    #[test]
    fn a_token_with_length_different_from_30_is_rejected() {
        let token = "a".repeat(40);
        assert_err!(Token::parse(token));

        let token = "a".repeat(20);
        assert_err!(Token::parse(token));
    }

    #[test]
    fn empty_string_is_rejected() {
        let token = "".to_string();
        assert_err!(Token::parse(token));
    }

    #[test]
    fn tokens_containing_invalid_char_are_rejected() {
        let token = "\"@#$$&/\\".to_string();
        assert_err!(Token::parse(token));
    }

    #[test]
    fn a_valid_token_is_parsed_successfully() {
        let token = "da39a3ee5e6b4b0d3255bfef956018".to_string();
        assert_ok!(Token::parse(token));
    }
}
