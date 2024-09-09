use validator::HasLen;

#[derive(Debug, thiserror::Error)]
#[error("{0} is not a valid validation code")]
pub struct ValidationCodeError(String);

#[derive(Debug)]
pub struct ValidationCode(String);

impl ValidationCode {
    pub fn parse(s: String) -> Result<ValidationCode, ValidationCodeError> {
        if s.length() == 6 && s.chars().all(|s| s.is_ascii_digit()) {
            Ok(ValidationCode(s))
        } else {
            Err(ValidationCodeError(s))
        }
    }
}

impl AsRef<str> for ValidationCode {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use claims::{assert_err, assert_ok};

    use super::ValidationCode;

    #[test]
    fn empty_code_is_rejected() {
        let code = "".to_string();
        assert_err!(ValidationCode::parse(code));
    }

    #[test]
    fn code_with_less_than_6_chars_is_rejected() {
        let code = "4".repeat(5);
        assert_err!(ValidationCode::parse(code));
    }

    #[test]
    fn code_with_more_than_6_chars_is_rejected() {
        let code = "4".repeat(7);
        assert_err!(ValidationCode::parse(code));
    }

    #[test]
    fn code_with_non_digits_is_rejected() {
        let code = "$#d@11".to_string();
        assert_err!(ValidationCode::parse(code));
    }

    #[test]
    fn code_only_with_6_digits_is_parsed_successfully() {
        let code = "152354".to_string();
        assert_ok!(ValidationCode::parse(code));
    }
}
