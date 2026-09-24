//! Compose and verify a policy sleeve in front of a component plugin.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod composition;

pub use composition::{LoadError, compose, sleeve_sha256, verify_composed_routing};

#[cfg(test)]
mod tests {
    use super::*;
    use wac_graph::{CompositionGraph, EncodeOptions, NodeId, types::Package};

    fn wire_to_other_sleeve(
        other_sleeve: Vec<u8>,
    ) -> impl FnOnce(&mut CompositionGraph, NodeId, NodeId, &[String]) -> Result<(), LoadError>
    {
        move |graph, _, plugin_instance, plugin_imports| {
            let package =
                Package::from_bytes("sleeve:other", None, other_sleeve, graph.types_mut())
                    .map_err(|error| LoadError::InvalidComponent(error.to_string()))?;
            let package = graph
                .register_package(package)
                .map_err(|error| LoadError::InvalidComponent(error.to_string()))?;
            let other_instance = graph.instantiate(package);
            for name in plugin_imports {
                let export = graph
                    .alias_instance_export(other_instance, name)
                    .map_err(|error| LoadError::InvalidComponent(error.to_string()))?;
                graph
                    .set_instantiation_argument(plugin_instance, name, export)
                    .map_err(|error| LoadError::InvalidComponent(error.to_string()))?;
            }
            Ok(())
        }
    }

    #[test]
    fn composes_every_plugin_import_through_the_sleeve() {
        let sleeve = std::fs::read(guest_build::passthrough_sleeve()).unwrap();
        let plugin = std::fs::read(guest_build::note_summary()).unwrap();
        let composed = compose(&plugin, &sleeve, sleeve_sha256(&sleeve)).unwrap();
        assert!(wasmparser::Parser::is_component(&composed));
    }

    #[test]
    fn rejects_an_altered_sleeve_before_decoding_it() {
        let sleeve = std::fs::read(guest_build::passthrough_sleeve()).unwrap();
        let plugin = std::fs::read(guest_build::note_summary()).unwrap();
        let expected = sleeve_sha256(&sleeve);
        let mut altered = sleeve;
        altered[0] ^= 1;
        let error = compose(&plugin, &altered, expected).unwrap_err();
        assert_eq!(error, LoadError::HashMismatch);
    }

    #[test]
    fn rejects_a_lookalike_interface_and_an_unrelated_host_import() {
        let sleeve = std::fs::read(guest_build::trace_sleeve()).unwrap();
        let digest = sleeve_sha256(&sleeve);
        for (plugin, expected) in [
            (guest_build::bypass(), "example:bypass/notes@0.1.0"),
            (
                guest_build::direct_import(),
                "example:direct-import/secrets@0.1.0",
            ),
        ] {
            let plugin = std::fs::read(plugin).unwrap();
            let error = compose(&plugin, &sleeve, digest).unwrap_err();
            assert_eq!(error, LoadError::UnsatisfiedImport(expected.into()));
        }
    }

    #[test]
    fn rejects_host_facing_plugin_imports_and_exports() {
        let sleeve = std::fs::read(guest_build::trace_sleeve()).unwrap();
        let digest = sleeve_sha256(&sleeve);
        let imported = std::fs::read(guest_build::platform_import()).unwrap();
        assert_eq!(
            compose(&imported, &sleeve, digest).unwrap_err(),
            LoadError::HostFacingImport("sleeve:platform/lifecycle@0.1.0".into())
        );
        let exported = std::fs::read(guest_build::platform_export()).unwrap();
        assert_eq!(
            compose(&exported, &sleeve, digest).unwrap_err(),
            LoadError::HostFacingExport("sleeve:platform/lifecycle@0.1.0".into())
        );
    }

    #[test]
    fn encoded_verifier_rejects_a_direct_plugin_instantiation_argument() {
        let sleeve = std::fs::read(guest_build::passthrough_sleeve()).unwrap();
        let plugin = std::fs::read(guest_build::note_summary()).unwrap();
        let mut graph = CompositionGraph::new();
        let sleeve_package =
            Package::from_bytes("sleeve:test", None, sleeve.clone(), graph.types_mut()).unwrap();
        let plugin_package =
            Package::from_bytes("plugin:test", None, plugin.clone(), graph.types_mut()).unwrap();
        let sleeve_id = graph.register_package(sleeve_package).unwrap();
        let plugin_id = graph.register_package(plugin_package).unwrap();
        graph.instantiate(sleeve_id);
        let plugin_instance = graph.instantiate(plugin_id);
        let export = graph
            .alias_instance_export(plugin_instance, "summarize")
            .unwrap();
        graph.export(export, "summarize").unwrap();
        let malicious = graph.encode(EncodeOptions::default()).unwrap();

        assert_eq!(
            verify_composed_routing(&malicious, &plugin, &sleeve).unwrap_err(),
            LoadError::ForwardedImport("example:notes/notes@0.1.0".into())
        );
    }

    #[test]
    fn encoded_verifier_rejects_an_alias_from_another_local_instance() {
        let sleeve = std::fs::read(guest_build::passthrough_sleeve()).unwrap();
        let other = std::fs::read(guest_build::trace_sleeve()).unwrap();
        let plugin = std::fs::read(guest_build::note_summary()).unwrap();
        let malicious =
            composition::encode_composition(&plugin, &sleeve, wire_to_other_sleeve(other)).unwrap();

        assert_eq!(
            verify_composed_routing(&malicious, &plugin, &sleeve).unwrap_err(),
            LoadError::ForwardedImport("example:notes/notes@0.1.0".into())
        );
    }

    #[test]
    fn compose_runs_the_encoded_routing_verifier() {
        let sleeve = std::fs::read(guest_build::passthrough_sleeve()).unwrap();
        let other = std::fs::read(guest_build::trace_sleeve()).unwrap();
        let plugin = std::fs::read(guest_build::note_summary()).unwrap();

        assert_eq!(
            composition::compose_with(
                &plugin,
                &sleeve,
                sleeve_sha256(&sleeve),
                wire_to_other_sleeve(other),
            )
            .unwrap_err(),
            LoadError::ForwardedImport("example:notes/notes@0.1.0".into())
        );
    }
}
