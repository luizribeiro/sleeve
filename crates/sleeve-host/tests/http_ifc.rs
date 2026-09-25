//! End-to-end HTTP decisions through both IFC policy chains.

use http_scenarios::{Authority, Expected, LocalServer, cases};
use sleeve_host::{HttpHost, sleeve_sha256};

#[tokio::test]
async fn applies_each_http_decision_under_both_policy_chains() {
    let server = LocalServer::start().unwrap();
    let plugin = std::fs::read(guest_build::http_scenarios()).unwrap();

    for (variant, sleeve_path) in [
        ("ifc", guest_build::ifc_sleeve()),
        ("trace-ifc", guest_build::trace_ifc_sleeve()),
    ] {
        let sleeve = std::fs::read(sleeve_path).unwrap();
        let host = HttpHost::new(
            [("secret".into(), "classified".into())],
            sleeve_sha256(&sleeve),
            8,
            [server.origin()],
        )
        .unwrap();

        for case in cases().unwrap() {
            let authority = match case.authority {
                Authority::Allowed => server.authority().to_owned(),
                Authority::Blocked => server.blocked_authority(),
                Authority::UppercaseAllowed => server.uppercase_authority(),
            };
            let invocation = format!("{variant}-{}", case.name);
            let attempt = host
                .run(
                    &plugin,
                    &sleeve,
                    &invocation,
                    case.input,
                    &authority,
                    case.body_size,
                )
                .await
                .unwrap();
            server.check().unwrap();
            match &case.expected {
                Expected::Returned(expected) => assert_eq!(
                    attempt.value.as_deref(),
                    Ok(expected.as_str()),
                    "{variant} {}",
                    case.name
                ),
                Expected::Trapped => assert!(
                    attempt.value.is_err(),
                    "{variant} {} unexpectedly returned",
                    case.name
                ),
            }
            if variant == "trace-ifc"
                && matches!(
                    case.name.as_str(),
                    "read-then-fetch" | "read-with-open-body" | "blocked-origin"
                )
            {
                assert!(
                    attempt.audit.iter().any(|line| line.ends_with("denied")),
                    "{} did not audit its denial: {:?}",
                    case.name,
                    attempt.audit
                );
            }
        }
    }
}
