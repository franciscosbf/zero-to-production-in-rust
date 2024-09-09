use super::{Email, EmailError};

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct CollaboratorEmailError(#[from] EmailError);

#[derive(Debug)]
pub struct CollaboratorEmail(Email);

impl std::fmt::Display for CollaboratorEmail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl CollaboratorEmail {
    pub fn parse(s: String) -> Result<CollaboratorEmail, CollaboratorEmailError> {
        Email::parse(s).map(Self).map_err(CollaboratorEmailError)
    }
}

impl AsRef<Email> for CollaboratorEmail {
    fn as_ref(&self) -> &Email {
        &self.0
    }
}
