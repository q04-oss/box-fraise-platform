#[tokio::main]
async fn main() -> anyhow::Result<()> {
    box_fraise_server::run().await
}
