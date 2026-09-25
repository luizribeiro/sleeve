//! Compares policy outcomes and task-local audit traces across both hosts.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use file_scenarios::cases as file_cases;
use http_scenarios::{Authority, LocalServer, cases as http_cases};
use jco_runner::{Attempt, Component};
use sleeve_host::{FileHost, FilePreopen, HttpHost, InvocationAttempt, sleeve_sha256};

#[tokio::test]
async fn classifies_every_cross_host_scenario() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let http_scenarios = repository.join("support/http-scenarios/scenarios.json");
    let file_scenarios = repository.join("support/file-scenarios/scenarios.json");

    let server = LocalServer::start().unwrap();
    let http_plugin = std::fs::read(guest_build::http_scenarios()).unwrap();
    let http_sleeve = std::fs::read(guest_build::trace_ifc_sleeve()).unwrap();
    let http_component = Component::transpile(&http_plugin, &http_sleeve).unwrap();
    let http_host = HttpHost::new(
        [("secret".into(), "classified".into())],
        sleeve_sha256(&http_sleeve),
        8,
        [server.origin()],
    )
    .unwrap();
    let mut wasmtime_http = Vec::new();
    for case in http_cases().unwrap() {
        let authority = authority(case.authority, &server);
        wasmtime_http.push((
            case.name.clone(),
            http_host
                .run(
                    &http_plugin,
                    &http_sleeve,
                    &case.name,
                    case.input,
                    &authority,
                    case.body_size,
                )
                .await
                .unwrap(),
        ));
    }
    let jco_http = http_component
        .run_http(
            &http_scenarios,
            server.authority(),
            &server.blocked_authority(),
            &server.uppercase_authority(),
            &server.origin(),
            None,
        )
        .unwrap();
    server.check().unwrap();

    let file_plugin = std::fs::read(guest_build::file_scenarios()).unwrap();
    let file_sleeve = std::fs::read(guest_build::trace_file_ifc_sleeve()).unwrap();
    let file_component = Component::transpile(&file_plugin, &file_sleeve).unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let public = temporary.path().join("public");
    let secret = temporary.path().join("secret");
    std::fs::create_dir_all(&public).unwrap();
    std::fs::create_dir_all(&secret).unwrap();
    std::fs::write(secret.join("note.txt"), "classified").unwrap();
    std::fs::write(public.join("existing.txt"), "public").unwrap();
    std::os::unix::fs::symlink("../secret/note.txt", public.join("secret-link")).unwrap();
    let file_host = FileHost::new(
        sleeve_sha256(&file_sleeve),
        [
            FilePreopen::new(&public, "public", "public"),
            FilePreopen::new(&secret, "secret", "secret"),
        ],
    )
    .unwrap();
    let mut wasmtime_files = Vec::new();
    for case in file_cases().unwrap() {
        wasmtime_files.push((
            case.name.clone(),
            file_host
                .run(&file_plugin, &file_sleeve, &case.name, case.input)
                .await
                .unwrap(),
        ));
    }
    let jco_files = file_component.run_files(&file_scenarios).unwrap();

    let http_divergences = divergences(&wasmtime_http, &jco_http);
    assert!(
        http_divergences.is_empty(),
        "HTTP divergences: {http_divergences:?}"
    );
    assert_eq!(
        divergences(&wasmtime_files, &jco_files),
        BTreeSet::from([
            "close-public-then-read-secret".to_owned(),
            "read-secret-then-write-public".to_owned(),
            "read-secret-then-write-secret".to_owned(),
            "truncate-only-after-secret".to_owned(),
            "write-only-after-secret".to_owned(),
        ])
    );
}

#[test]
fn accepts_scheduler_local_drop_timing() {
    let early = strings(&[
        "invocation start task",
        "call 1 example:test/resources.make",
        "return 1 ok handles=7",
        "handle dropped 7",
        "call 2 example:test/work.run",
        "return 2 ok",
        "invocation end task returned",
    ]);
    let late = strings(&[
        "invocation start task",
        "call 1 example:test/resources.make",
        "return 1 ok handles=12",
        "call 2 example:test/work.run",
        "return 2 ok",
        "handle dropped 12",
        "invocation end task returned",
    ]);
    assert_eq!(compare_trace(&early, &late), Ok(()));
}

#[tokio::test]
#[ignore = "jco 1.35.0: RuntimeError: wasm trap: deadlock detected: event loop cannot make further progress"]
async fn plugin_trap_rejects_export_while_anchor_is_pending() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scenarios = repository.join("support/http-scenarios/scenarios.json");
    let server = LocalServer::start().unwrap();
    let plugin = std::fs::read(guest_build::http_scenarios()).unwrap();
    let sleeve = std::fs::read(guest_build::trace_ifc_sleeve()).unwrap();
    let component = Component::transpile(&plugin, &sleeve).unwrap();
    let attempts = component
        .run_http(
            &scenarios,
            server.authority(),
            &server.blocked_authority(),
            &server.uppercase_authority(),
            &server.origin(),
            Some("read-with-open-body"),
        )
        .unwrap();
    let attempt = &attempts[0];
    assert!(
        attempt
            .audit
            .last()
            .is_some_and(|event| event.starts_with("return ") && event.ends_with(" denied"))
    );
    assert_eq!(attempt.error.as_deref(), Some("RuntimeError: unreachable"));
}

fn authority(authority: Authority, server: &LocalServer) -> String {
    match authority {
        Authority::Allowed => server.authority().to_owned(),
        Authority::Blocked => server.blocked_authority(),
        Authority::UppercaseAllowed => server.uppercase_authority(),
    }
}

fn divergences(wasmtime: &[(String, InvocationAttempt)], jco: &[Attempt]) -> BTreeSet<String> {
    wasmtime
        .iter()
        .zip(jco)
        .filter_map(|((name, wasmtime), jco)| {
            let outcome_matches = match (&wasmtime.value, jco.status.as_str()) {
                (Ok(value), "returned") => jco.value.as_ref() == Some(value),
                (Err(_), "trapped") => true,
                _ => false,
            };
            let trace = compare_trace(&wasmtime.audit, &jco.audit);
            let differs = name != &jco.name || !outcome_matches || trace.is_err();
            differs.then(|| name.clone())
        })
        .collect()
}

fn compare_trace(left: &[String], right: &[String]) -> Result<(), String> {
    let left = NormalizedTrace::new(left)?;
    let right = NormalizedTrace::new(right)?;
    if left.non_drop != right.non_drop {
        return Err("non-drop events differ".into());
    }
    if left.drops != right.drops {
        return Err("drop sets differ".into());
    }
    Ok(())
}

struct NormalizedTrace {
    non_drop: Vec<String>,
    drops: BTreeSet<u64>,
}

impl NormalizedTrace {
    fn new(trace: &[String]) -> Result<Self, String> {
        let mut handles = BTreeMap::new();
        let mut next = 1_u64;
        let mut produced = BTreeMap::new();
        for (index, event) in trace.iter().enumerate() {
            if let Some(ids) = event.split_once(" handles=").map(|(_, ids)| ids) {
                for id in ids.split(',').map(parse_handle) {
                    let id = id?;
                    handles.entry(id).or_insert_with(|| take_next(&mut next));
                    produced.insert(id, index);
                }
            }
            if let Some(id) = event.strip_prefix("channel opened ").and_then(first_handle) {
                handles.entry(id).or_insert_with(|| take_next(&mut next));
            }
        }

        let invocation_end = trace
            .iter()
            .position(|event| event.starts_with("invocation end "));
        let mut drops = BTreeSet::new();
        let mut non_drop = Vec::new();
        for (index, event) in trace.iter().enumerate() {
            if let Some(id) = event.strip_prefix("handle dropped ").map(parse_handle) {
                let id = id?;
                let normalized = normalized(&handles, id)?;
                let produced_at = produced
                    .get(&id)
                    .ok_or_else(|| format!("handle {id} dropped without a return"))?;
                if index <= *produced_at || invocation_end.is_some_and(|end| index >= end) {
                    return Err(format!("handle {id} dropped outside its lifetime"));
                }
                if !drops.insert(normalized) {
                    return Err(format!("handle {id} dropped more than once"));
                }
            } else {
                non_drop.push(normalize_event(event, &handles)?);
            }
        }
        Ok(Self { non_drop, drops })
    }
}

fn normalize_event(event: &str, handles: &BTreeMap<u64, u64>) -> Result<String, String> {
    if let Some((prefix, ids)) = event.split_once(" handles=") {
        let ids = ids
            .split(',')
            .map(parse_handle)
            .map(|id| id.and_then(|id| normalized(handles, id)))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(format!(
            "{prefix} handles={}",
            ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",")
        ));
    }
    for prefix in ["channel opened ", "channel closed "] {
        if let Some(rest) = event.strip_prefix(prefix) {
            let id = first_handle(rest).ok_or_else(|| format!("invalid channel event: {event}"))?;
            let suffix = &rest[id.to_string().len()..];
            return Ok(format!("{prefix}{}{suffix}", normalized(handles, id)?));
        }
    }
    Ok(event.to_owned())
}

fn first_handle(value: &str) -> Option<u64> {
    value.split_whitespace().next()?.parse().ok()
}

fn parse_handle(value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|error| format!("invalid handle `{value}`: {error}"))
}

fn normalized(handles: &BTreeMap<u64, u64>, id: u64) -> Result<u64, String> {
    handles
        .get(&id)
        .copied()
        .ok_or_else(|| format!("handle {id} used before production"))
}

fn take_next(next: &mut u64) -> u64 {
    let value = *next;
    *next += 1;
    value
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}
