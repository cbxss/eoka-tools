mod mcp;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    mcp::run_server().await
}
