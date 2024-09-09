use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

use crate::helpers::{assert_is_redirect_to, extract_validation_code, spawn_app};

#[tokio::test]
async fn you_must_be_logged_in_to_access_send_invitation() {
    let test_app = spawn_app().await;

    let body = serde_json::json!({
        "email": "ursula_le_guin@gmail.com",
    });

    let response = test_app.invite_collaborator(&body).await;

    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn you_must_be_admin_to_send_invitation() {
    let test_app = spawn_app().await;

    let collaborator = test_app.create_collaborator().await;

    let response = test_app
        .post_login(&serde_json::json!({
            "username": &collaborator.username,
            "password": &collaborator.password,
        }))
        .await;

    assert_is_redirect_to(&response, "/admin/dashboard");

    let html_page = test_app.get_admin_dashboard_html().await;

    assert!(html_page.contains(&format!("Welcome {}", collaborator.username)));

    let body = serde_json::json!({
        "email": "ursula_le_guin@gmail.com",
    });

    let response = test_app.invite_collaborator(&body).await;

    assert_eq!(405, response.status().as_u16());
}

#[tokio::test]
async fn invite_returns_a_200_for_valid_form_data() {
    let test_app = spawn_app().await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&test_app.email_server)
        .await;

    test_app
        .post_login(&serde_json::json!({
            "username": &test_app.test_user.username,
            "password": &test_app.test_user.password,
        }))
        .await;

    let body = serde_json::json!({
        "email": "ursula_le_guin@gmail.com",
    });

    let response = test_app.invite_collaborator(&body).await;

    assert_eq!(200, response.status().as_u16());
}

#[tokio::test]
async fn invite_returns_a_validation_code_of_6_digits() {
    let test_app = spawn_app().await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&test_app.email_server)
        .await;

    test_app
        .post_login(&serde_json::json!({
            "username": &test_app.test_user.username,
            "password": &test_app.test_user.password,
        }))
        .await;

    let body = serde_json::json!({
        "email": "ursula_le_guin@gmail.com",
    });

    let response = test_app.invite_collaborator(&body).await;

    let validation_code = extract_validation_code(response).await;

    assert_eq!(validation_code.len(), 6);
    assert!(validation_code.chars().all(|c| c.is_ascii_digit()));
}

#[tokio::test]
async fn invite_persists_invitation_token() {
    let test_app = spawn_app().await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&test_app.email_server)
        .await;

    test_app
        .post_login(&serde_json::json!({
            "username": &test_app.test_user.username,
            "password": &test_app.test_user.password,
        }))
        .await;

    let body = serde_json::json!({
        "email": "ursula_le_guin@gmail.com",
    });

    let response = test_app.invite_collaborator(&body).await;

    let validation_code = extract_validation_code(response).await;

    let saved = sqlx::query!("SELECT invitation_token, validation_code from invitation_tokens")
        .fetch_one(&test_app.db_pool)
        .await
        .expect("Failed to retrieve stored token");

    assert_eq!(validation_code, saved.validation_code);
}

#[tokio::test]
async fn invite_returns_400_if_email_is_missing() {
    let test_app = spawn_app().await;

    test_app
        .post_login(&serde_json::json!({
            "username": &test_app.test_user.username,
            "password": &test_app.test_user.password,
        }))
        .await;

    let body = serde_json::json!({});

    let response = test_app.invite_collaborator(&body).await;

    assert_eq!(400, response.status().as_u16());
}

#[tokio::test]
async fn invite_returns_400_if_email_is_present_but_missing() {
    let test_app = spawn_app().await;

    test_app
        .post_login(&serde_json::json!({
            "username": &test_app.test_user.username,
            "password": &test_app.test_user.password,
        }))
        .await;

    let test_cases = vec![
        (serde_json::json!({"email": ""}), "empty email"),
        (
            serde_json::json!({"email": "invalid-email"}),
            "invalid email",
        ),
    ];

    for (body, description) in test_cases {
        let response = test_app.invite_collaborator(&body).await;

        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not return 400 Bad Request when the payload was {description}."
        );
    }
}

#[tokio::test]
async fn invite_sends_an_invitation_for_valid_data() {
    let test_app = spawn_app().await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&test_app.email_server)
        .await;

    test_app
        .post_login(&serde_json::json!({
            "username": &test_app.test_user.username,
            "password": &test_app.test_user.password,
        }))
        .await;

    let body = serde_json::json!({
        "email": "ursula_le_guin@gmail.com",
    });

    test_app.invite_collaborator(&body).await;
}

#[tokio::test]
async fn invite_sends_an_invitation_with_a_link() {
    let test_app = spawn_app().await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&test_app.email_server)
        .await;

    test_app
        .post_login(&serde_json::json!({
            "username": &test_app.test_user.username,
            "password": &test_app.test_user.password,
        }))
        .await;

    let body = serde_json::json!({
        "email": "ursula_le_guin@gmail.com",
    });

    test_app.invite_collaborator(&body).await;

    let email_request = &test_app.email_server.received_requests().await.unwrap()[0];
    let invitation_link = test_app.get_links(email_request);

    assert_eq!(invitation_link.html, invitation_link.plain_text);
}
