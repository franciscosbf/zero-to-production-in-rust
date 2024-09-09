use actix_web::{http::StatusCode, web, HttpResponse, ResponseError};
use anyhow::Context;
use rand::{thread_rng, Rng};
use sqlx::{PgPool, Postgres, Transaction};

use crate::{
    domain::{CollaboratorEmail, CollaboratorEmailError, NewCollaborator},
    email_client::EmailClient,
    routes::error_chain_fmt,
    session_state::TypedSession,
    startup::ApplicationBaseUrl,
    template::{self, render_collaborator_invitation},
    user_role::UserRole,
};

#[derive(thiserror::Error)]
pub enum CollaboratorParseError {
    #[error(transparent)]
    InvalidEmail(CollaboratorEmailError),
}

impl std::fmt::Debug for CollaboratorParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

pub struct StoreCollaboratorTokenError(sqlx::Error);

impl std::error::Error for StoreCollaboratorTokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl std::fmt::Display for StoreCollaboratorTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A database error was encountered while trying to store collaborator token"
        )
    }
}

impl std::fmt::Debug for StoreCollaboratorTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl actix_web::ResponseError for StoreCollaboratorTokenError {}

#[derive(thiserror::Error)]
pub enum InviteError {
    #[error("Restricted operation")]
    NonAdminError,
    #[error("{0}")]
    ValidationError(CollaboratorParseError),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for InviteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for InviteError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        match self {
            InviteError::NonAdminError => StatusCode::METHOD_NOT_ALLOWED,
            InviteError::ValidationError(_) => StatusCode::BAD_REQUEST,
            InviteError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(serde::Deserialize)]
pub struct CollaboratorFormData {
    email: String,
}

impl TryFrom<CollaboratorFormData> for NewCollaborator {
    type Error = CollaboratorParseError;

    fn try_from(value: CollaboratorFormData) -> Result<Self, Self::Error> {
        let email =
            CollaboratorEmail::parse(value.email).map_err(CollaboratorParseError::InvalidEmail)?;

        Ok(Self { email })
    }
}

fn generate_invitation_token() -> String {
    let mut rng = thread_rng();

    std::iter::repeat_with(|| rng.sample(rand::distributions::Alphanumeric))
        .map(char::from)
        .take(30)
        .collect()
}

fn generate_validation_code() -> String {
    let mut rng = thread_rng();

    std::iter::repeat_with(|| rng.sample(rand::distributions::Uniform::new_inclusive(0, 9)))
        .map(|d| char::from_digit(d, 10).unwrap())
        .take(6)
        .collect()
}

#[tracing::instrument(
    name = "Saving new collaborator invitation",
    skip(transaction, invitation_token, validation_code)
)]
async fn insert_collaborator_token(
    transaction: &mut Transaction<'_, Postgres>,
    invitation_token: &str,
    validation_code: &str,
) -> Result<(), StoreCollaboratorTokenError> {
    sqlx::query!(
        r#"
        INSERT INTO invitation_tokens (invitation_token, validation_code)
        VALUES ($1, $2)
        "#,
        invitation_token,
        validation_code,
    )
    .execute(&mut **transaction)
    .await
    .map_err(StoreCollaboratorTokenError)?;

    Ok(())
}

#[tracing::instrument(
    name = "Render collaborator invitation message",
    skip(base_url, invitation_token)
)]
fn build_collaborator_invitation_template(
    base_url: &str,
    invitation_token: &str,
) -> Result<template::CollaboratorInvitation, tera::Error> {
    let invitiation_link = format!(
        "{}/collaborator?invitation_token={}",
        base_url, invitation_token,
    );

    render_collaborator_invitation(&invitiation_link)
}

#[tracing::instrument(
    name = "Send invitation email",
    skip(email_client, new_collaborator, template)
)]
async fn send_invitation_email(
    email_client: &EmailClient,
    new_collaborator: NewCollaborator,
    template: template::CollaboratorInvitation,
) -> Result<(), reqwest::Error> {
    email_client
        .send_email(
            new_collaborator.email.as_ref(),
            "Welcome!",
            &template.html,
            &template.text,
        )
        .await
}

#[tracing::instrument(
    name = "Inviting new collaborator",
    skip(form, session, pool, email_client, base_url),
    fields(collaborator_email = %form.email)
)]
pub async fn invite_collaborator(
    form: web::Form<CollaboratorFormData>,
    session: TypedSession,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
) -> Result<HttpResponse, InviteError> {
    if session
        .get_user_role()
        .context("Failed to get user rule from its session")?
        .unwrap()
        != UserRole::Admin
    {
        return Err(InviteError::NonAdminError);
    }

    let new_collaborator: NewCollaborator =
        form.0.try_into().map_err(InviteError::ValidationError)?;

    let invitation_token = generate_invitation_token();
    let validation_code = generate_validation_code();

    let mut transaction = pool
        .begin()
        .await
        .context("Failed to aquire a Postgres connection from the pool")?;

    insert_collaborator_token(&mut transaction, &invitation_token, &validation_code)
        .await
        .context("Failed to insert invitation token for new collaborator")?;

    transaction
        .commit()
        .await
        .context("Failed to commit SQL transaction to store new collaborator token")?;

    let template = build_collaborator_invitation_template(&base_url.0, &invitation_token)
        .context("Failed to generate email template for invitation")?;
    send_invitation_email(&email_client, new_collaborator, template)
        .await
        .context("Failed to send invitation email")?;

    Ok(HttpResponse::Ok().json(serde_json::json!({"validation_code": validation_code})))
}
