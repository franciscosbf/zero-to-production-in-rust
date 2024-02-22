use std::net::TcpListener;

use newsletter::{
    configuration::{get_configuration, DatabaseSettings},
    email_client::EmailClient,
    telemetry::{get_subscriber, init_subscriber},
};
use once_cell::sync::Lazy;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;

static TRACING: Lazy<()> = Lazy::new(|| {
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber("test".into(), "debug".into(), std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber("test".into(), "debug".into(), std::io::sink);
        init_subscriber(subscriber);
    }
});

async fn configure_database(config: &DatabaseSettings) -> PgPool {
    let mut connection = PgConnection::connect_with(&config.without_db())
        .await
        .expect("Failed to connect to Postgres.");
    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("Failed to create database.");

    let connection_pool = PgPool::connect_with(config.with_db())
        .await
        .expect("Failed to connect to Postgres.");
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the database");

    connection_pool
}

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
}

pub async fn spawn_app() -> TestApp {
    Lazy::force(&TRACING);

    let mut configuration = get_configuration().expect("Failed to read configuration.");

    let host = configuration.application.host;
    let listener = TcpListener::bind(format!("{host}:0")).expect("Failed to bind random port.");
    let port = listener.local_addr().unwrap().port();
    let address = format!("http://{host}:{port}");

    configuration.database.database_name = Uuid::new_v4().to_string();
    let connection_pool = configure_database(&configuration.database).await;

    let sender_email = configuration
        .email_client
        .sender()
        .expect("Invalid sender email address.");
    let base_url = configuration
        .email_client
        .url()
        .expect("Invalid email base url.");
    let timeout = configuration.email_client.timeout();
    let email_client = EmailClient::new(
        base_url,
        sender_email,
        configuration.email_client.authorization_token,
        timeout,
    );

    let server = newsletter::startup::run(listener, connection_pool.clone(), email_client)
        .expect("Fail to bind address");
    #[allow(clippy::let_underscore_future)]
    let _ = tokio::spawn(server);

    TestApp {
        address,
        db_pool: connection_pool,
    }
}
