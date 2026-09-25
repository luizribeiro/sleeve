//! Shows information-flow decisions around HTTP calls.

use http_scenarios::{LocalServer, Scenario};
use sleeve_host::HttpHost;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let server = LocalServer::start()?;
    let sleeve = std::fs::read(guest_build::trace_ifc_sleeve())?;
    let plugin = std::fs::read(guest_build::http_scenarios())?;
    let host = HttpHost::new(
        [("secret".into(), "classified".into())],
        sleeve_host::sleeve_sha256(&sleeve),
        8,
        [server.origin()],
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    for (name, scenario) in [
        ("read note, then fetch", Scenario::ReadThenFetch),
        ("fetch, then read note", Scenario::FetchThenRead),
    ] {
        println!("scenario: {name}");
        let attempt = host
            .run(
                &plugin,
                &sleeve,
                name,
                scenario as u8,
                server.authority(),
                0,
            )
            .await?;
        for event in attempt.audit {
            println!("  {event}");
        }
        match attempt.value {
            Ok(value) => println!("  decision: allowed ({value})"),
            Err(_) => println!("  decision: denied"),
        }
    }
    Ok(())
}
