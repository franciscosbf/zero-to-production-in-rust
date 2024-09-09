use super::{token::TokenError, Token};

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct SubscriptionTokenError(#[from] TokenError);

#[derive(Debug)]
pub struct SubscriptionToken(Token);

impl SubscriptionToken {
    pub fn parse(s: String) -> Result<SubscriptionToken, SubscriptionTokenError> {
        Token::parse(s).map(Self).map_err(SubscriptionTokenError)
    }
}

impl AsRef<str> for SubscriptionToken {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}
