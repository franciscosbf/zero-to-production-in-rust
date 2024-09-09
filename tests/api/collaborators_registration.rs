use uuid::Uuid;
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

use crate::helpers::{assert_is_redirect_to, extract_validation_code, spawn_app};

#[tokio::test]
async fn registrations_without_tokens_are_rejected_with_a_400_when_requesting_registration_form() {
    let test_app = spawn_app().await;

    let response = reqwest::get(&format!("{}/collaborator", test_app.address))
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn registrations_without_stored_token_are_rejected_with_a_401_when_requesting_registration_form(
) {
    let test_app = spawn_app().await;

    let response = test_app
        .get_collaborator_registration("da39a3ee5e6b4b0d3255bfef956018")
        .await;

    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn registration_form_is_successfully_returned_when_requested_with_a_valid_invitation_token() {
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

    let invitation_token = test_app.extract_invitation_token().await;

    let response = test_app
        .get_collaborator_registration(&invitation_token)
        .await;

    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn invitation_token_and_validation_code_must_be_valid() {
    let test_app = spawn_app().await;
    let test_cases = vec![
        (
            serde_json::json!({
                "invitation_token": "invalid",
                "validation_code": "123456",
                "username": "collaborator",
                "password": Uuid::new_v4().to_string(),
            }),
            "invalid invitation token",
        ),
        (
            serde_json::json!({
                "invitation_token": "da39a3ee5e6b4b0d3255bfef956018",
                "validation_code": "24g5t45h",
                "username": "collaborator",
                "password": Uuid::new_v4().to_string(),
            }),
            "invalid validation code",
        ),
    ];

    for (invalid_body, error_message) in test_cases {
        let response = test_app.register_collaborator(&invalid_body).await;

        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail with 400 Bad Request when the payload was {error_message}."
        );
    }
}

#[tokio::test]
async fn password_must_be_valid() {
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

    let invitation_token = test_app.extract_invitation_token().await;
    let validation_code = extract_validation_code(response).await;

    let invalid_body = serde_json::json!({
        "invitation_token": invitation_token,
        "validation_code": validation_code,
        "username": "collaborator",
        "password": "oi",
    });

    let response = test_app.register_collaborator(&invalid_body).await;

    assert_is_redirect_to(&response, "/collaborator");

    let html_page = test_app
        .get_collaborator_registration_html(&invitation_token)
        .await;

    assert!(html_page
        .contains("<p><i>New password must contain at least 8 and up to 64 characters.</i></p>"))
}

#[tokio::test]
async fn invitation_token_must_exist() {
    let test_app = spawn_app().await;

    let invalid_body = serde_json::json!({
        "invitation_token": "da39a3ee5e6b4b0d3255bfef956018",
        "validation_code": "123456",
        "username": "collaborator",
        "password": Uuid::new_v4().to_string(),
    });

    let response = test_app.register_collaborator(&invalid_body).await;

    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn new_collaborator_must_contain_a_unique_username() {
    let test_app = spawn_app().await;

    let collaborator = test_app.create_collaborator().await;

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

    let invitation_token = test_app.extract_invitation_token().await;
    let validation_code = extract_validation_code(response).await;

    let invalid_body = serde_json::json!({
        "invitation_token": invitation_token,
        "validation_code": validation_code,
        "username": collaborator.username,
        "password": Uuid::new_v4().to_string(),
    });

    let response = test_app.register_collaborator(&invalid_body).await;

    assert_is_redirect_to(&response, "/collaborator");

    let html_page = test_app
        .get_collaborator_registration_html(&invitation_token)
        .await;

    assert!(html_page.contains(&format!(
        "<p><i>Username \"{}\" is already in use.</i></p>",
        collaborator.username
    )))
}

#[tokio::test]
async fn new_collaborator_is_registered_with_success() {
    let test_app = spawn_app().await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
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

    let invitation_token = test_app.extract_invitation_token().await;
    let validation_code = extract_validation_code(response).await;

    let collaborator_username = "collaborator";
    let collaborator_password = Uuid::new_v4().to_string();

    let invalid_body = serde_json::json!({
        "invitation_token": invitation_token,
        "validation_code": validation_code,
        "username": collaborator_username,
        "password": collaborator_password,
    });

    let response = test_app.register_collaborator(&invalid_body).await;

    assert_eq!(response.status().as_u16(), 200);

    sqlx::query!(
        r#"SELECT username FROM users WHERE username = $1"#,
        collaborator_username
    )
    .fetch_one(&test_app.db_pool)
    .await
    .expect("Failed to fetch collaborator");

    let login_body = serde_json::json!({
        "username": collaborator_username,
        "password": collaborator_password,
    });
    let response = test_app.post_login(&login_body).await;

    assert_is_redirect_to(&response, "/admin/dashboard");

    let html_page = test_app.get_admin_dashboard_html().await;
    assert!(html_page.contains(&format!("Welcome {}", collaborator_username)));
}
