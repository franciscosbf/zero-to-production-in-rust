use std::net::TcpListener;

fn spawn_app() -> String {
    let listener = TcpListener::bind("localhost:0").expect("Failed to bind random port.");
    let port = listener.local_addr().unwrap().port();
    let server = newsletter::run(listener).expect("Fail to bind address");
    #[allow(clippy::let_underscore_future)]
    let _ = tokio::spawn(server);
    format!("http://localhost:{port}")
}

#[tokio::test]
async fn health_check_works() {
    let address = spawn_app();
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{address}/health_check"))
        .send()
        .await
        .expect("Failed to execute request.");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}
