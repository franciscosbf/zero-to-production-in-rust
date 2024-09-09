use actix_web::{http::StatusCode, web, HttpResponse, ResponseError};
use actix_web_flash_messages::FlashMessage;
use anyhow::Context;
use secrecy::{ExposeSecret, Secret};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    authentication::compute_password_hash,
    domain::{InvitationToken, InvitationTokenError, ValidationCode, ValidationCodeError},
    routes::error_chain_fmt,
    util::see_other,
};

#[derive(serde::Deserialize)]
pub struct FormData {
    invitation_token: String,
    validation_code: String,
    username: String,
    password: Secret<String>,
}

#[derive(thiserror::Error)]
pub enum CollaboratorRegistrationError {
    #[error("{0}")]
    TokenValidationError(InvitationTokenError),
    #[error("{0}")]
    CodeValidationError(ValidationCodeError),
    #[error("Registration not authorized")]
    MissingRegistrationError,
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for CollaboratorRegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for CollaboratorRegistrationError {
    fn status_code(&self) -> StatusCode {
        match self {
            CollaboratorRegistrationError::TokenValidationError(_)
            | CollaboratorRegistrationError::CodeValidationError(_) => StatusCode::BAD_REQUEST,
            CollaboratorRegistrationError::MissingRegistrationError => StatusCode::UNAUTHORIZED,
            CollaboratorRegistrationError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[tracing::instrument(name = "Remove invitation token", skip(invitation_token))]
async fn remove_invitation_token(
    transaction: &mut Transaction<'_, Postgres>,
    invitation_token: InvitationToken,
    validation_code: ValidationCode,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        r#"
        DELETE FROM invitation_tokens
        WHERE invitation_token = $1 AND
            validation_code = $2
        RETURNING 1 as contained
        "#,
        invitation_token.as_ref(),
        validation_code.as_ref(),
    )
    .fetch_optional(&mut **transaction)
    .await
    .map(|r| r.is_some())
}

#[tracing::instrument(
    name = "Register new collaborator",
    skip(transaction, password_hash),
    fields(user_id=tracing::field::Empty, username=tracing::field::Empty)
)]
async fn insert_collaborator(
    transaction: &mut Transaction<'_, Postgres>,
    username: &str,
    password_hash: Secret<String>,
) -> Result<bool, sqlx::Error> {
    let user_id = Uuid::new_v4();

    let result = sqlx::query!(
        r#"
        INSERT INTO users (user_id, username, password_hash, role)
        VALUES ($1, $2, $3, 'collaborator')
        "#,
        user_id,
        username,
        password_hash.expose_secret()
    )
    .execute(&mut **transaction)
    .await;

    match result {
        Ok(_) => {
            tracing::Span::current()
                .record("user_id", tracing::field::display(&user_id))
                .record("username", tracing::field::display(username));

            Ok(true)
        }
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Ok(false),
        Err(error) => Err(error),
    }
}

#[tracing::instrument(name = "Register collaborator", skip(form, pool))]
pub async fn register_collaborator(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, CollaboratorRegistrationError> {
    let form_data = form.into_inner();

    let invitation_token = InvitationToken::parse(form_data.invitation_token)
        .map_err(CollaboratorRegistrationError::TokenValidationError)?;

    let validation_code = ValidationCode::parse(form_data.validation_code)
        .map_err(CollaboratorRegistrationError::CodeValidationError)?;

    if !(8..=64).contains(&form_data.password.expose_secret().len()) {
        FlashMessage::error("New password must contain at least 8 and up to 64 characters.").send();

        return Ok(see_other("/collaborator"));
    }

    let password_hash =
        compute_password_hash(form_data.password).context("Failed to compute password hash")?;

    let mut transaction = pool
        .begin()
        .await
        .context("Failed to aquire a Postgres connection from the pool")?;

    if !remove_invitation_token(&mut transaction, invitation_token, validation_code)
        .await
        .context("Failed to remove invitation token")?
    {
        return Err(CollaboratorRegistrationError::MissingRegistrationError);
    }

    if !insert_collaborator(&mut transaction, &form_data.username, password_hash)
        .await
        .context("Failed to insert new collaborator")?
    {
        FlashMessage::error(format!(
            "Username \"{}\" is already in use.",
            form_data.username
        ))
        .send();

        return Ok(see_other("/collaborator"));
    }

    transaction
        .commit()
        .await
        .context("Failed to commit SQL transaction to store new collaborator")?;

    Ok(HttpResponse::Ok().finish())
}
