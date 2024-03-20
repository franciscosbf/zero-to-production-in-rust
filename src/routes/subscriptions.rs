use actix_web::{web, HttpResponse, ResponseError};
use chrono::Utc;
use rand::{thread_rng, Rng};
use reqwest::StatusCode;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
    startup::ApplicationBaseUrl,
    template::{self, render_subscription_confirmation},
};

fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}", e)?;

    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }

    Ok(())
}

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
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),
    #[error("Failed to acquire a Postgres connectiion from the pool")]
    PoolError(#[source] sqlx::Error),
    #[error("Failed to store the confirmation token for a new subscriber")]
    StoreTokenError(#[from] StoreTokenError),
    #[error("Failed to send confirmation email")]
    SendEmailError(#[from] reqwest::Error),
    #[error("Failed to insert new subscriber in the database")]
    InsertSubscriberError(#[source] sqlx::Error),
    #[error("Failed to commit SQL transaction to store a new subscriber")]
    TransactionCommitError(#[source] sqlx::Error),
    #[error("Failed to get confirmation token from subscriber")]
    GetTokenError(#[source] sqlx::Error),
    #[error("Subscriber is already registered")]
    RepeatedSubscriberError,
}

impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> reqwest::StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
            SubscribeError::RepeatedSubscriberError => StatusCode::NOT_ACCEPTABLE,
            SubscribeError::StoreTokenError(_)
            | SubscribeError::SendEmailError(_)
            | SubscribeError::PoolError(_)
            | SubscribeError::InsertSubscriberError(_)
            | SubscribeError::TransactionCommitError(_)
            | SubscribeError::GetTokenError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(value: FormData) -> Result<Self, Self::Error> {
        let email = SubscriberEmail::parse(value.email)?;
        let name = SubscriberName::parse(value.name)?;

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
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);

        StoreTokenError(e)
    })?;

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
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

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
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })
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

    render_subscription_confirmation(&confirmation_link).map_err(|e| {
        tracing::error!("Failed to render subscription confirmation: {:?}", e);
        e
    })
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
            new_subscriber.email,
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
) -> Result<HttpResponse, actix_web::Error> {
    let new_subscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;

    let mut transaction = pool
        .begin()
        .await
        .map_err(SubscribeError::TransactionCommitError)?;

    let subscription_state = insert_susbscriber(&mut transaction, &new_subscriber)
        .await
        .map_err(SubscribeError::InsertSubscriberError)?;

    let subscription_token = match subscription_state {
        SubscriptionState::Confirmed => Err(SubscribeError::RepeatedSubscriberError)?,
        SubscriptionState::Inserted(subscriber_id) => {
            let subscription_token = generate_subscription_token();

            store_token(&mut transaction, subscriber_id, &subscription_token).await?;

            subscription_token
        }
        SubscriptionState::Pending(subscriber_id) => {
            get_subscriber_confirmation_token(&mut transaction, subscriber_id)
                .await
                .map_err(SubscribeError::GetTokenError)?
        }
    };

    let template = match build_confirmation_email_template(&base_url.0, &subscription_token) {
        Ok(template) => template,
        Err(_) => return Ok(HttpResponse::InternalServerError().finish()),
    };
    send_confirmation_email(&email_client, new_subscriber, template)
        .await
        .map_err(SubscribeError::SendEmailError)?;

    transaction
        .commit()
        .await
        .map_err(SubscribeError::TransactionCommitError)?;

    Ok(HttpResponse::Ok().finish())
}
