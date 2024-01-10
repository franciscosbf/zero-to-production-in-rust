use std::net::TcpListener;

use newsletter::startup::run;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    run(TcpListener::bind("localhost:8000")?)?.await
}
