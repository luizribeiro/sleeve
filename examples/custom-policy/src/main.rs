//! Builds and runs a sleeve with an example-owned policy.

use http_scenarios::LocalServer;
use sleeve_host::HttpHost;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let server = LocalServer::start()?;
    let sleeve = std::fs::read(env!("CUSTOM_POLICY_SLEEVE"))?;
    let plugin = std::fs::read(guest_build::http_scenarios())?;
    let host = HttpHost::new(
        [],
        sleeve_host::sleeve_sha256(&sleeve),
        1024,
        [server.origin()],
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    let result = host
        .run(&plugin, &sleeve, "four requests", 8, server.authority(), 4)
        .await?;
    println!("plugin: {}", result.value.map_err(anyhow::Error::msg)?);
    for summary in result.audit {
        println!("{summary}");
    }
    server.check()?;
    Ok(())
}
