use actix_web::{http::StatusCode, web, HttpResponse, ResponseError};
use anyhow::Context;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::{SubscriptionToken, SubscriptionTokenError};

use super::error_chain_fmt;

#[derive(serde::Deserialize)]
pub struct SubscriptionConfirmationParameters {
    subscription_token: String,
}

impl TryFrom<SubscriptionConfirmationParameters> for SubscriptionToken {
    type Error = SubscriptionTokenError;

    fn try_from(value: SubscriptionConfirmationParameters) -> Result<Self, Self::Error> {
        SubscriptionToken::parse(value.subscription_token)
    }
}

#[derive(thiserror::Error)]
pub enum SubscriptionConfirmationError {
    #[error("{0}")]
    ValidationError(SubscriptionTokenError),
    #[error("Confirmation not authorized")]
    MissingConfirmationError,
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for SubscriptionConfirmationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscriptionConfirmationError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscriptionConfirmationError::ValidationError(_) => StatusCode::BAD_REQUEST,
            SubscriptionConfirmationError::MissingConfirmationError => StatusCode::UNAUTHORIZED,
            SubscriptionConfirmationError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[tracing::instrument(
    name = "Delete possible pending subscriber confirmation",
    skip(transaction, subscription_token)
)]
pub async fn delete_possible_pending_subscriber_confirmation(
    transaction: &mut Transaction<'_, Postgres>,
    subscription_token: SubscriptionToken,
) -> Result<Option<Uuid>, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        DELETE from subscription_tokens
        WHERE subscription_token = $1
        RETURNING subscriber_id
        "#,
        subscription_token.as_ref()
    )
    .fetch_optional(&mut **transaction)
    .await?;

    Ok(result.map(|r| r.subscriber_id))
}

#[tracing::instrument(
    name = "Mark subscriber as confirmed",
    skip(transaction, subscriber_id)
)]
pub async fn confirm_subscriber(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        UPDATE subscriptions
        SET status = 'confirmed'
        WHERE id = $1
        "#,
        &subscriber_id
    )
    .execute(&mut **transaction)
    .await?;

    Ok(())
}

#[tracing::instrument(name = "Confirm pending subscriber", skip(parameters, pool))]
pub async fn confirm(
    parameters: web::Query<SubscriptionConfirmationParameters>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, SubscriptionConfirmationError> {
    let subscription_token = parameters
        .0
        .try_into()
        .map_err(SubscriptionConfirmationError::ValidationError)?;

    let mut transaction = pool
        .begin()
        .await
        .context("Failed to aquire a Postgres connection from the pool")?;

    let subscriber_id =
        delete_possible_pending_subscriber_confirmation(&mut transaction, subscription_token)
            .await
            .context("Failed to delete possible pending subscriber confirmation")?
            .ok_or(SubscriptionConfirmationError::MissingConfirmationError)?;

    confirm_subscriber(&mut transaction, subscriber_id)
        .await
        .context("Failed to confirm new subscriber")?;

    transaction
        .commit()
        .await
        .context("Failed to commit SQL transaction to store new subscriber")?;

    Ok(HttpResponse::Ok().finish())
}
