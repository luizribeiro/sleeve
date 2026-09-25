//! End-to-end filesystem decisions through both IFC policy chains.

use file_scenarios::CASES;
use sleeve_host::{FileHost, FilePreopen, LoadError, compose, sleeve_sha256};

#[tokio::test]
async fn applies_each_file_decision_under_both_policy_chains() {
    let plugin = std::fs::read(guest_build::file_scenarios()).unwrap();

    for (variant, sleeve_path) in [
        ("ifc", guest_build::file_ifc_sleeve()),
        ("trace-ifc", guest_build::trace_file_ifc_sleeve()),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let public = temporary.path().join("public");
        let secret = temporary.path().join("secret");
        std::fs::create_dir_all(&public).unwrap();
        std::fs::create_dir_all(&secret).unwrap();
        std::fs::write(secret.join("note.txt"), "classified").unwrap();
        std::fs::write(public.join("existing.txt"), "public").unwrap();
        std::os::unix::fs::symlink("../secret/note.txt", public.join("secret-link")).unwrap();

        let sleeve = std::fs::read(sleeve_path).unwrap();
        let host = FileHost::new(
            sleeve_sha256(&sleeve),
            [
                FilePreopen::new(&public, "public", "public"),
                FilePreopen::new(&secret, "secret", "secret"),
            ],
        )
        .unwrap();

        for case in CASES {
            let attempt = host
                .run(
                    &plugin,
                    &sleeve,
                    &format!("{variant}-{}", case.name),
                    case.scenario as u8,
                )
                .await
                .unwrap();
            assert_eq!(
                attempt.value.as_deref(),
                Ok(case.expected),
                "{variant} {}: {:?}",
                case.name,
                attempt.audit
            );
            if variant == "trace-ifc" && case.policy_denial {
                assert!(
                    attempt.audit.iter().any(|event| event.ends_with("denied")),
                    "{} did not audit its denial: {:?}",
                    case.name,
                    attempt.audit
                );
            }
        }

        assert_eq!(
            std::fs::read_to_string(secret.join("report.txt")).unwrap(),
            "classified"
        );
        assert!(!public.join("report.txt").exists());
    }
}

#[test]
fn rejects_nested_preopens_at_construction() {
    let temporary = tempfile::tempdir().unwrap();
    let public = temporary.path().join("public");
    let secret = public.join("secret");
    std::fs::create_dir_all(&secret).unwrap();

    let error = FileHost::new(
        [0; 32],
        [
            FilePreopen::new(&public, "public", "public"),
            FilePreopen::new(&secret, "secret", "secret"),
        ],
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("filesystem preopens overlap"));
}

#[test]
fn rejects_a_plugin_that_imports_an_unwrapped_filesystem_method() {
    let sleeve = std::fs::read(guest_build::file_ifc_sleeve()).unwrap();
    let plugin = std::fs::read(guest_build::filesystem_bypass()).unwrap();

    assert!(matches!(
        compose(&plugin, &sleeve, sleeve_sha256(&sleeve)),
        Err(LoadError::UnsatisfiedImport(name)) if name.ends_with(".[method]descriptor.get-type")
    ));
}

#[test]
fn composes_sync_stream_constructors_generated_from_upstream_wit() {
    let sleeve = std::fs::read(guest_build::file_ifc_sleeve()).unwrap();
    let plugin = std::fs::read(guest_build::filesystem_upstream()).unwrap();

    compose(&plugin, &sleeve, sleeve_sha256(&sleeve)).unwrap();
}
