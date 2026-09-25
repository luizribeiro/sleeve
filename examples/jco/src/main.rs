//! Shows the HTTP information-flow decisions through jco under Node.

use std::path::Path;

use http_scenarios::LocalServer;
use jco_runner::Component;

fn main() -> anyhow::Result<()> {
    let server = LocalServer::start()?;
    let sleeve = std::fs::read(guest_build::trace_ifc_sleeve())?;
    let plugin = std::fs::read(guest_build::http_scenarios())?;
    let component = Component::transpile(&plugin, &sleeve)?;
    let scenarios =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../support/http-scenarios/scenarios.json");

    for (name, selected) in [
        ("read note, then fetch", "read-then-fetch"),
        ("fetch, then read note", "fetch-then-read"),
    ] {
        println!("scenario: {name}");
        let attempt = component
            .run_http(
                &scenarios,
                server.authority(),
                &server.blocked_authority(),
                &server.uppercase_authority(),
                &server.origin(),
                Some(selected),
            )?
            .remove(0);
        for event in attempt.audit {
            println!("  {event}");
        }
        match attempt.value {
            Some(value) if attempt.status == "returned" => {
                println!("  decision: allowed ({value})");
            }
            _ => println!("  decision: denied"),
        }
    }
    server.check()?;
    Ok(())
}
