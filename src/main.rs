use anyhow::Result;

#[tokio::main]
/// Starts the Tokio runtime and delegates to the library entrypoint.
async fn main() -> Result<()> {
  ocppsim::run().await
}
