use newsletter::email_client::EmailClient;
use sqlx::postgres::PgPoolOptions;
use std::net::TcpListener;

use newsletter::configuration::get_configuration;
use newsletter::startup::run;
use newsletter::telemetry::{get_subscriber, init_subscriber};

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let subscriber = get_subscriber("newsletter".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

    let configuration = get_configuration().expect("Failed to read configuration.");

    let connection_pool = PgPoolOptions::new().connect_lazy_with(configuration.database.with_db());

    let sender_email = configuration
        .email_client
        .sender()
        .expect("Invalid sender email address.");
    let base_url = configuration
        .email_client
        .url()
        .expect("Invalid email base url.");
    let email_client = EmailClient::new(
        base_url,
        sender_email,
        configuration.email_client.authorization_token,
    );

    let listener = TcpListener::bind(configuration.application.address())?;

    run(listener, connection_pool, email_client)?.await
}
