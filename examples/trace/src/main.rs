//! Runs a note-summary plugin through the tracing sleeve.

use sleeve_host::Host;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let sleeve = std::fs::read(guest_build::trace_sleeve())?;
    let approved_sleeve = sleeve_host::sleeve_sha256(&sleeve);
    let host = Host::new(
        [
            ("today".into(), "Call Ada at 10".into()),
            ("project".into(), "The launch is Friday".into()),
        ],
        approved_sleeve,
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let plugin = std::fs::read(guest_build::note_summary())?;
    let result = host
        .summarize(&plugin, &sleeve, "note-summary", "today", "project")
        .await?;

    for record in result.audit {
        println!("{record}");
    }
    println!("result: {}", result.value);
    Ok(())
}
