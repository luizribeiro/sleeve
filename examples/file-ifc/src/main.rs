//! Shows information-flow decisions around labeled filesystem preopens.

use file_scenarios::Scenario;
use sleeve_host::{FileHost, FilePreopen};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let public = temporary.path().join("public");
    let secret = temporary.path().join("secret");
    std::fs::create_dir_all(&public)?;
    std::fs::create_dir_all(&secret)?;
    std::fs::write(secret.join("note.txt"), "classified")?;

    let sleeve = std::fs::read(guest_build::trace_file_ifc_sleeve())?;
    let plugin = std::fs::read(guest_build::file_scenarios())?;
    let host = FileHost::new(
        sleeve_host::sleeve_sha256(&sleeve),
        [
            FilePreopen::new(&public, "public", "public"),
            FilePreopen::new(&secret, "secret", "secret"),
        ],
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    for (name, scenario) in [
        (
            "read secret while a public writer is open",
            Scenario::OpenPublicThenReadSecret,
        ),
        (
            "close the public writer before reading secret",
            Scenario::ClosePublicThenReadSecret,
        ),
    ] {
        println!("scenario: {name}");
        let attempt = host.run(&plugin, &sleeve, name, scenario as u8).await?;
        for event in attempt.audit {
            println!("  {event}");
        }
        match attempt.value {
            Ok(value) => println!("  result: {value}"),
            Err(_) => println!("  result: trapped"),
        }
    }
    Ok(())
}
