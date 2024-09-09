use actix_web::{
    http::{header::ContentType, StatusCode},
    web, HttpResponse, ResponseError,
};
use actix_web_flash_messages::IncomingFlashMessages;
use anyhow::Context;
use sqlx::PgPool;
use std::fmt::Write;

use crate::{
    domain::{InvitationToken, InvitationTokenError},
    routes::error_chain_fmt,
};

#[derive(serde::Deserialize)]
pub struct Parameters {
    invitation_token: String,
}

#[derive(thiserror::Error)]
pub enum CollaboratorRegistrationFormError {
    #[error("{0}")]
    ValidationError(InvitationTokenError),
    #[error("Invitation not authorized")]
    MissingInvitationError,
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for CollaboratorRegistrationFormError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for CollaboratorRegistrationFormError {
    fn status_code(&self) -> StatusCode {
        match self {
            CollaboratorRegistrationFormError::ValidationError(_) => StatusCode::BAD_REQUEST,
            CollaboratorRegistrationFormError::MissingInvitationError => StatusCode::UNAUTHORIZED,
            CollaboratorRegistrationFormError::UnexpectedError(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

impl TryFrom<Parameters> for InvitationToken {
    type Error = InvitationTokenError;

    fn try_from(value: Parameters) -> Result<Self, Self::Error> {
        InvitationToken::parse(value.invitation_token)
    }
}

pub async fn contains_invitation_token(
    token: InvitationToken,
    pool: &PgPool,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        r#"
        SELECT 1 as contains
        FROM invitation_tokens
        WHERE invitation_token = $1
        "#,
        token.as_ref()
    )
    .fetch_optional(pool)
    .await
    .map(|r| r.is_some())
}

pub async fn register_collaborator_form(
    parameters: web::Query<Parameters>,
    pool: web::Data<PgPool>,
    flash_messages: IncomingFlashMessages,
) -> Result<HttpResponse, CollaboratorRegistrationFormError> {
    let invitation_token = parameters
        .0
        .try_into()
        .map_err(CollaboratorRegistrationFormError::ValidationError)?;

    if !contains_invitation_token(invitation_token, &pool)
        .await
        .context("Failed to check invitation token")?
    {
        return Err(CollaboratorRegistrationFormError::MissingInvitationError);
    }

    let mut error_html = String::new();
    for m in flash_messages.iter() {
        writeln!(error_html, "<p><i>{}</i></p>", m.content()).unwrap();
    }

    let response = HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"<!DOCTYPE html>
<html lang="en">
    <head>
        <meta http-equiv="content-type" content="text/html; charset=utf-8">
        <title>Collaborator registration</title>
    </head>
    <body>
        {error_html}
        <form action="/collaborator/register" method="post">
            <label>
                Username
                <input type="text" placeholder="Enter Username" name="username">
            </label>
            <label>
                Password
                <input type="password" placeholder="Enter Password" name="password">
            </label>
            <label>
                Validation Code
                <input type="text" placeholder="Enter Validation Code" name="validation_code" pattern="[0-9]{{6}}" required>
            </label>
            <label>
                <input id="invitation_token" type="hidden" name="invitation_token">
            </label>
            <button type="submit">Register</button>
        </form>
    </body>
    <script>
        const invitation_token = (new URLSearchParams(window.location.search)).get("invitation_token");
        document.getElementById("invitation_token").value = invitation_token || "";
    </script>
</html>"#,
        ));

    Ok(response)
}
