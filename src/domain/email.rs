use validator::validate_email;

#[derive(Debug, thiserror::Error)]
pub enum EmailError {
    #[error("Invalid email format")]
    InvalidFormat,
}

#[derive(Debug)]
pub struct Email(String);

impl std::fmt::Display for Email {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Email {
    pub fn parse(s: String) -> Result<Email, EmailError> {
        if validate_email(&s) {
            Ok(Self(s))
        } else {
            Err(EmailError::InvalidFormat)
        }
    }
}

impl AsRef<str> for Email {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use claims::assert_err;
    use fake::{faker::internet::en::SafeEmail, Fake};
    use rand::SeedableRng;

    use super::Email;

    #[derive(Debug, Clone)]
    struct ValidEmailFixture(pub String);

    impl quickcheck::Arbitrary for ValidEmailFixture {
        fn arbitrary(_: &mut quickcheck::Gen) -> Self {
            // Workaround since I can't pass quickcheck::Gen struct to fake_with_rng.
            let mut rng = rand::rngs::SmallRng::from_entropy();

            let email = SafeEmail().fake_with_rng(&mut rng);
            Self(email)
        }
    }

    #[quickcheck_macros::quickcheck]
    fn valid_emails_are_parsed_successfully(valid_email: ValidEmailFixture) -> bool {
        Email::parse(valid_email.0).is_ok()
    }

    #[test]
    fn empty_string_is_rejected() {
        let email = "".to_string();
        assert_err!(Email::parse(email));
    }

    #[test]
    fn email_missing_at_symbol_is_rejected() {
        let email = "franciscodomain.com".to_string();
        assert_err!(Email::parse(email));
    }

    #[test]
    fn email_missing_subject_is_rejected() {
        let email = "@domain.com".to_string();
        assert_err!(Email::parse(email));
    }
}
