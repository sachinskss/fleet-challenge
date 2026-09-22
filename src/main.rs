#[tokio::main]
async fn main() {
    if let Err(error) = fleet_challenge::run().await {
        eprintln!("server failed: {error}");
        std::process::exit(1);
    }
}
