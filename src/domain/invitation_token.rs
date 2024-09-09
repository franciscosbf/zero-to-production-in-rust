use super::{token::TokenError, Token};

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct InvitationTokenError(#[from] TokenError);

#[derive(Debug)]
pub struct InvitationToken(Token);

impl InvitationToken {
    pub fn parse(s: String) -> Result<InvitationToken, InvitationTokenError> {
        Token::parse(s).map(Self).map_err(InvitationTokenError)
    }
}

impl AsRef<str> for InvitationToken {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}
