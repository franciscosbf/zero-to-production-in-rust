use actix_web::{http::StatusCode, web, HttpResponse, ResponseError};
use anyhow::Context;
use chrono::Utc;
use rand::{thread_rng, Rng};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    domain::{
        NewSubscriber, SubscriberEmail, SubscriberEmailError, SubscriberName, SubscriberNameError,
    },
    email_client::EmailClient,
    startup::ApplicationBaseUrl,
    template::{self, render_subscription_confirmation},
};

use super::error_chain_fmt;

pub struct StoreTokenError(sqlx::Error);

impl std::error::Error for StoreTokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl std::fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A database error was encountered while trying to store a subscription token"
        )
    }
}

impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl actix_web::ResponseError for StoreTokenError {}

#[derive(thiserror::Error)]
pub enum ParseError {
    #[error(transparent)]
    InvalidName(SubscriberNameError),
    #[error(transparent)]
    InvalidEmail(SubscriberEmailError),
}

impl std::fmt::Debug for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(ParseError),
    #[error("Duplicated subscriber")]
    DuplicatedSubscriberError,
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
            SubscribeError::DuplicatedSubscriberError => StatusCode::NOT_ACCEPTABLE,
            SubscribeError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = ParseError;

    fn try_from(value: FormData) -> Result<Self, Self::Error> {
        let email = SubscriberEmail::parse(value.email).map_err(ParseError::InvalidEmail)?;
        let name = SubscriberName::parse(value.name).map_err(ParseError::InvalidName)?;

        Ok(NewSubscriber { email, name })
    }
}

fn generate_subscription_token() -> String {
    let mut rng = thread_rng();

    std::iter::repeat_with(|| rng.sample(rand::distributions::Alphanumeric))
        .map(char::from)
        .take(30)
        .collect()
}

#[tracing::instrument(
    name = "Store subscription token in the database",
    skip(transaction, subscription_token)
)]
pub async fn store_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), StoreTokenError> {
    sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id,
    )
    .execute(&mut **transaction)
    .await
    .map_err(StoreTokenError)?;

    Ok(())
}

#[derive(Debug)]
pub enum SubscriptionState {
    Inserted(Uuid),
    Pending(Uuid),
    Confirmed,
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(transaction, new_subscriber)
)]
pub async fn insert_susbscriber(
    transaction: &mut Transaction<'_, Postgres>,
    new_subscriber: &NewSubscriber,
) -> Result<SubscriptionState, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();

    let result = sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'pending_confirmation')
        -- idk a better way to this without using only one query...
        ON CONFLICT (email) DO UPDATE SET status = subscriptions.status
        RETURNING id, status
        "#,
        subscriber_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(),
        Utc::now()
    )
    .fetch_one(&mut **transaction)
    .await?;

    let status = if subscriber_id == result.id {
        SubscriptionState::Inserted(subscriber_id)
    } else if result.status == "pending_confirmation" {
        SubscriptionState::Pending(result.id)
    } else {
        SubscriptionState::Confirmed
    };

    Ok(status)
}

#[tracing::instrument(
    name = "Fetch subscription token of pending subscriber",
    skip(transaction, subscriber_id)
)]
pub async fn get_subscriber_confirmation_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
) -> Result<String, sqlx::Error> {
    sqlx::query!(
        r#"
        SELECT subscription_token
        FROM subscription_tokens
        WHERE subscriber_id = $1
        "#,
        subscriber_id,
    )
    .fetch_one(&mut **transaction)
    .await
    .map(|result| result.subscription_token)
}

#[tracing::instrument(
    name = "Render subscription confirmation message",
    skip(base_url, subscription_token)
)]
fn build_confirmation_email_template(
    base_url: &str,
    subscription_token: &str,
) -> Result<template::SubcriptionConfirmation, tera::Error> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token,
    );

    render_subscription_confirmation(&confirmation_link)
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(email_client, new_subscriber, template)
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    template: template::SubcriptionConfirmation,
) -> Result<(), reqwest::Error> {
    email_client
        .send_email(
            &new_subscriber.email,
            "Welcome!",
            &template.html,
            &template.text,
        )
        .await
}

#[tracing::instrument(
    name = "Adding a new susbscriber",
    skip(form, pool, email_client, base_url),
    fields(
        susbscriber_email = %form.email,
        susbscriber_name = %form.name
    )
)]
pub async fn subscribe(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
) -> Result<HttpResponse, SubscribeError> {
    let new_subscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;

    let mut transaction = pool
        .begin()
        .await
        .context("Failed to aquire a Postgres connection from the pool")?;

    let subscription_state = insert_susbscriber(&mut transaction, &new_subscriber)
        .await
        .context("Failed to insert new subscriber in the database")?;

    let subscription_token = match subscription_state {
        SubscriptionState::Confirmed => Err(SubscribeError::DuplicatedSubscriberError)?,
        SubscriptionState::Inserted(subscriber_id) => {
            let subscription_token = generate_subscription_token();

            store_token(&mut transaction, subscriber_id, &subscription_token)
                .await
                .context("Failed to store the confirmation token for a new subscriber")?;

            subscription_token
        }
        SubscriptionState::Pending(subscriber_id) => {
            get_subscriber_confirmation_token(&mut transaction, subscriber_id)
                .await
                .context("Failed to retrieve subscriber confirmation token")?
        }
    };

    transaction
        .commit()
        .await
        .context("Failed to commit SQL transaction to store new subscriber")?;

    let template = build_confirmation_email_template(&base_url.0, &subscription_token)
        .context("Failed to generate email template for confirmation email")?;
    send_confirmation_email(&email_client, new_subscriber, template)
        .await
        .context("Failed to send confirmation email")?;

    Ok(HttpResponse::Ok().finish())
}
