use claims::assert_none;
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

use crate::helpers::spawn_app;

#[tokio::test]
async fn confirmations_without_tokens_are_rejected_with_a_400() {
    let test_app = spawn_app().await;

    let response = reqwest::get(&format!("{}/subscriptions/confirm", test_app.address))
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn the_link_returned_by_subscribe_returns_a_200_if_called() {
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    test_app.post_subscription(body.into()).await;
    let email_request = &test_app.email_server.received_requests().await.unwrap()[0];
    let confirmation_link = test_app.get_links(email_request);

    let response = reqwest::get(confirmation_link.html).await.unwrap();

    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn clicking_on_the_confirmation_link_confirms_subscriber() {
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    test_app.post_subscription(body.into()).await;
    let email_request = &test_app.email_server.received_requests().await.unwrap()[0];
    let confirmation_link = test_app.get_links(email_request);

    reqwest::get(confirmation_link.html)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions",)
        .fetch_one(&test_app.db_pool)
        .await
        .expect("Failed to fetch saved subscriptions");

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, "confirmed");
}

#[tokio::test]
async fn subscribe_returns_a_406_when_trying_to_subscribe_with_an_already_confirmed_email() {
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    test_app.post_subscription(body.into()).await;
    let email_request = &test_app.email_server.received_requests().await.unwrap()[0];
    let confirmation_link = test_app.get_links(email_request);

    reqwest::get(confirmation_link.html)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let response = test_app.post_subscription(body.into()).await;

    assert_eq!(response.status().as_u16(), 406);
}

#[tokio::test]
// async fn clicking_on_the_confirmation_link_more_than_once_returns_401() {
async fn clicking_on_the_confirmation_link_removes_subscription_token() {
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    test_app.post_subscription(body.into()).await;
    let email_request = &test_app.email_server.received_requests().await.unwrap()[0];
    let confirmation_link = test_app.get_links(email_request);

    reqwest::get(confirmation_link.html.clone())
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let subscription_token = confirmation_link
        .html
        .query()
        .unwrap()
        .split('=')
        .nth(1)
        .unwrap();

    let saved = sqlx::query!(
        r#"
        SELECT *
        FROM subscription_tokens
        WHERE subscription_token = $1
        "#,
        subscription_token
    )
    .fetch_optional(&test_app.db_pool)
    .await
    .expect("Failed to fetch saved subscriptions");

    assert_none!(saved);
}

#[tokio::test]
async fn clicking_on_the_confirmation_link_more_than_once_returns_401() {
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    test_app.post_subscription(body.into()).await;
    let email_request = &test_app.email_server.received_requests().await.unwrap()[0];
    let confirmation_link = test_app.get_links(email_request);

    reqwest::get(confirmation_link.html.clone())
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let result = reqwest::get(confirmation_link.plain_text).await.unwrap();

    assert_eq!(result.status().as_u16(), 401);
}

#[tokio::test]
async fn confirm_returns_a_400_when_token_is_invalid() {
    let test_app = spawn_app().await;
    let query_token = "token=\"@#$$&/\\".to_string();
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    test_app.post_subscription(body.into()).await;
    let email_request = &test_app.email_server.received_requests().await.unwrap()[0];
    let mut confirmation_link = test_app.get_links(email_request);

    confirmation_link.html.set_query(Some(&query_token));

    let result = reqwest::get(confirmation_link.html).await.unwrap();

    assert_eq!(result.status().as_u16(), 400);
}
