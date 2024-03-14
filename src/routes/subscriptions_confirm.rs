use actix_web::{web, HttpResponse};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::SubscriptionToken;

#[derive(serde::Deserialize)]
pub struct Parameters {
    subscription_token: String,
}

impl TryFrom<Parameters> for SubscriptionToken {
    type Error = String;

    fn try_from(value: Parameters) -> Result<Self, Self::Error> {
        SubscriptionToken::parse(value.subscription_token)
    }
}

#[tracing::instrument(
    name = "Get subscriber_id from token",
    skip(transaction, subscription_token)
)]
pub async fn get_subscriber_id_from_token(
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
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

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
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;

    Ok(())
}

#[tracing::instrument(name = "Confirm pending subscriber", skip(parameters, pool))]
pub async fn confirm(parameters: web::Query<Parameters>, pool: web::Data<PgPool>) -> HttpResponse {
    let subscription_token = match parameters.0.try_into() {
        Ok(subscription_token) => subscription_token,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    let mut transaction = match pool.begin().await {
        Ok(transaction) => transaction,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let id = match get_subscriber_id_from_token(&mut transaction, subscription_token).await {
        Ok(id) => id,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let subscriber_id = match id {
        None => return HttpResponse::Unauthorized().finish(),
        Some(subscriber_id) => subscriber_id,
    };

    if confirm_subscriber(&mut transaction, subscriber_id)
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
