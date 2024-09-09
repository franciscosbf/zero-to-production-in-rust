use super::{Email, EmailError};

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct SubscriberEmailError(EmailError);

#[derive(Debug)]
pub struct SubscriberEmail(Email);

impl std::fmt::Display for SubscriberEmail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl SubscriberEmail {
    pub fn parse(s: String) -> Result<SubscriberEmail, SubscriberEmailError> {
        Email::parse(s).map(Self).map_err(SubscriberEmailError)
    }
}

impl AsRef<Email> for SubscriberEmail {
    fn as_ref(&self) -> &Email {
        &self.0
    }
}
