use actix_web::{web, HttpResponse};
use chrono::Utc;
use rand::{thread_rng, Rng};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
    startup::ApplicationBaseUrl,
};

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
) -> Result<(), sqlx::Error> {
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
        e
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
    name = "Send a confirmation email to a new subscriber",
    skip(email_client, new_subscriber)
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token,
    );
    let html_content = format!(
        "Welcome to our newsletter!<br/>\
                Click <a href=\"{}\">here<a/> to confirm your subscription.",
        confirmation_link
    );

    let text_content = format!(
        "Welcome to our newsletter!\n\
                Visit {} to confirm your subscription.",
        confirmation_link
    );

    email_client
        .send_email(
            new_subscriber.email,
            "Welcome!",
            &html_content,
            &text_content,
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
) -> HttpResponse {
    let new_subscriber = match form.0.try_into() {
        Ok(subscriber) => subscriber,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    let mut transaction = match pool.begin().await {
        Ok(transaction) => transaction,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let subscription_state = match insert_susbscriber(&mut transaction, &new_subscriber).await {
        Ok(subscriber_id) => subscriber_id,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    // println!("{:?}", subscription_state);

    let subscription_token = match subscription_state {
        SubscriptionState::Confirmed => return HttpResponse::NotAcceptable().finish(),
        SubscriptionState::Inserted(subscriber_id) => {
            let subscription_token = generate_subscription_token();

            if store_token(&mut transaction, subscriber_id, &subscription_token)
                .await
                .is_err()
            {
                return HttpResponse::InternalServerError().finish();
            }

            subscription_token
        }
        SubscriptionState::Pending(subscriber_id) => {
            match get_subscriber_confirmation_token(&mut transaction, subscriber_id).await {
                Ok(subscription_token) => subscription_token,
                Err(_) => return HttpResponse::InternalServerError().finish(),
            }
        }
    };

    if send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.0,
        &subscription_token,
    )
    .await
    .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    if transaction.commit().await.is_err() {
        return HttpResponse::InternalServerError().finish();
    }

    HttpResponse::Ok().finish()
}
