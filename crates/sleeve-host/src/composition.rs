use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{self, Display};
use std::ops::Range;

use sha2::{Digest, Sha256};
use wac_graph::{CompositionGraph, EncodeOptions, NodeId, types::Package};
use wasmparser::{
    ComponentAlias, ComponentExternalKind, ComponentInstance, ComponentOuterAliasKind,
    ComponentTypeRef, Parser, Payload,
};

const PLUGIN_INTERFACES: &[&str] = &["example:notes/notes@0.1.0"];

enum InstanceOrigin {
    Other,
    ExportAlias { instance: u32, name: String },
}

struct Instantiation {
    component: u32,
    instance: u32,
    arguments: Vec<InstantiationArgument>,
}

struct InstantiationArgument {
    name: String,
    kind: ComponentExternalKind,
    index: u32,
}

struct RoutingIndex {
    next_component: u32,
    instances: Vec<InstanceOrigin>,
    plugin_component: Option<u32>,
    sleeve_component: Option<u32>,
    instantiations: Vec<Instantiation>,
}

impl RoutingIndex {
    fn parse(bytes: &[u8], plugin_bytes: &[u8], sleeve_bytes: &[u8]) -> Result<Self, LoadError> {
        let mut index = Self {
            next_component: 0,
            instances: Vec::new(),
            plugin_component: None,
            sleeve_component: None,
            instantiations: Vec::new(),
        };
        let mut depth = 0_u32;
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.map_err(invalid)? {
                Payload::ModuleSection { .. } => depth += 1,
                Payload::ComponentSection {
                    unchecked_range, ..
                } => {
                    if depth == 0 {
                        index.record_embedded(
                            bytes,
                            unchecked_range,
                            plugin_bytes,
                            sleeve_bytes,
                        )?;
                    }
                    depth += 1;
                }
                Payload::End(_) if depth > 0 => depth -= 1,
                Payload::ComponentImportSection(section) if depth == 0 => {
                    for import in section {
                        index.record_import(import.map_err(invalid)?.ty)?;
                    }
                }
                Payload::ComponentInstanceSection(section) if depth == 0 => {
                    for instance in section {
                        index.record_instance(instance.map_err(invalid)?)?;
                    }
                }
                Payload::ComponentAliasSection(section) if depth == 0 => {
                    for alias in section {
                        index.record_alias(&alias.map_err(invalid)?)?;
                    }
                }
                _ => {}
            }
        }
        Ok(index)
    }

    fn record_embedded(
        &mut self,
        bytes: &[u8],
        range: Range<u64>,
        plugin_bytes: &[u8],
        sleeve_bytes: &[u8],
    ) -> Result<(), LoadError> {
        let start = usize::try_from(range.start).map_err(invalid)?;
        let end = usize::try_from(range.end).map_err(invalid)?;
        let nested = bytes.get(start..end).ok_or_else(|| {
            LoadError::InvalidComponent("embedded component range is invalid".into())
        })?;
        let component = claim_index(&mut self.next_component, "component")?;
        if nested == plugin_bytes {
            record_component(&mut self.plugin_component, component, "plugin")?;
        }
        if nested == sleeve_bytes {
            record_component(&mut self.sleeve_component, component, "sleeve")?;
        }
        Ok(())
    }

    fn record_import(&mut self, ty: ComponentTypeRef) -> Result<(), LoadError> {
        match ty {
            ComponentTypeRef::Instance(_) => self.instances.push(InstanceOrigin::Other),
            ComponentTypeRef::Component(_) => {
                claim_index(&mut self.next_component, "component")?;
            }
            _ => {}
        }
        Ok(())
    }

    fn record_instance(&mut self, instance: ComponentInstance<'_>) -> Result<(), LoadError> {
        let instance_index = u32::try_from(self.instances.len()).map_err(invalid)?;
        if let ComponentInstance::Instantiate {
            component_index,
            args,
        } = instance
        {
            self.instantiations.push(Instantiation {
                component: component_index,
                instance: instance_index,
                arguments: args
                    .iter()
                    .map(|argument| InstantiationArgument {
                        name: argument.name.into(),
                        kind: argument.kind,
                        index: argument.index,
                    })
                    .collect(),
            });
        }
        self.instances.push(InstanceOrigin::Other);
        Ok(())
    }

    fn record_alias(&mut self, alias: &ComponentAlias<'_>) -> Result<(), LoadError> {
        match alias {
            ComponentAlias::InstanceExport {
                kind: ComponentExternalKind::Instance,
                instance_index,
                name,
            } => {
                let source = usize::try_from(*instance_index).map_err(invalid)?;
                if source >= self.instances.len() {
                    return Err(LoadError::InvalidComponent(
                        "instance alias source is not defined".into(),
                    ));
                }
                self.instances.push(InstanceOrigin::ExportAlias {
                    instance: *instance_index,
                    name: (*name).into(),
                });
            }
            ComponentAlias::InstanceExport {
                kind: ComponentExternalKind::Component,
                ..
            }
            | ComponentAlias::Outer {
                kind: ComponentOuterAliasKind::Component,
                ..
            } => {
                claim_index(&mut self.next_component, "component")?;
            }
            _ => {}
        }
        Ok(())
    }

    fn verify(&self, plugin_imports: &[String]) -> Result<(), LoadError> {
        let plugin_component = self.plugin_component.ok_or_else(|| {
            LoadError::InvalidComponent("composition does not embed the selected plugin".into())
        })?;
        let sleeve_instance = self.selected_sleeve_instance()?;
        let mut found = false;
        for instantiation in self
            .instantiations
            .iter()
            .filter(|instantiation| instantiation.component == plugin_component)
        {
            found = true;
            for name in plugin_imports {
                self.verify_argument(instantiation, name, sleeve_instance)?;
            }
        }
        if !found {
            return Err(LoadError::InvalidComponent(
                "composition does not instantiate the selected plugin".into(),
            ));
        }
        Ok(())
    }

    fn selected_sleeve_instance(&self) -> Result<u32, LoadError> {
        let sleeve_component = self.sleeve_component.ok_or_else(|| {
            LoadError::InvalidComponent("composition does not embed the selected sleeve".into())
        })?;
        let mut instances = self
            .instantiations
            .iter()
            .filter(|instantiation| instantiation.component == sleeve_component)
            .map(|instantiation| instantiation.instance);
        let instance = instances.next().ok_or_else(|| {
            LoadError::InvalidComponent(
                "composition does not instantiate the selected sleeve".into(),
            )
        })?;
        if instances.next().is_some() {
            return Err(LoadError::InvalidComponent(
                "composition instantiates the selected sleeve more than once".into(),
            ));
        }
        Ok(instance)
    }

    fn verify_argument(
        &self,
        instantiation: &Instantiation,
        name: &str,
        sleeve_instance: u32,
    ) -> Result<(), LoadError> {
        let argument = instantiation
            .arguments
            .iter()
            .find(|argument| argument.name == name)
            .ok_or_else(|| LoadError::ForwardedImport(name.into()))?;
        if argument.kind != ComponentExternalKind::Instance {
            return Err(LoadError::ForwardedImport(name.into()));
        }
        let index = usize::try_from(argument.index).map_err(invalid)?;
        let Some(InstanceOrigin::ExportAlias {
            instance,
            name: export,
        }) = self.instances.get(index)
        else {
            return Err(LoadError::ForwardedImport(name.into()));
        };
        if *instance != sleeve_instance || export != name {
            return Err(LoadError::ForwardedImport(name.into()));
        }
        Ok(())
    }
}

/// A load-time refusal raised before a composed plugin can be instantiated.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum LoadError {
    /// The sleeve bytes differ from the digest selected by the embedder.
    HashMismatch,
    /// The plugin asks for an interface the sleeve does not export.
    UnsatisfiedImport(String),
    /// The plugin asks for an interface reserved for communication with the host.
    HostFacingImport(String),
    /// The plugin exports a name reserved for communication with the host.
    HostFacingExport(String),
    /// The encoded graph does not route a plugin import through the approved sleeve.
    ForwardedImport(String),
    /// Component decoding or graph encoding failed.
    InvalidComponent(String),
}

impl Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HashMismatch => {
                formatter.write_str("sleeve SHA-256 does not match the pinned digest")
            }
            Self::UnsatisfiedImport(name) => {
                write!(
                    formatter,
                    "plugin import `{name}` is not exported by the sleeve"
                )
            }
            Self::HostFacingImport(name) => {
                write!(formatter, "plugin import `{name}` is host-facing")
            }
            Self::HostFacingExport(name) => {
                write!(formatter, "plugin export `{name}` is host-facing")
            }
            Self::ForwardedImport(name) => {
                write!(
                    formatter,
                    "plugin import `{name}` is not routed through the approved sleeve"
                )
            }
            Self::InvalidComponent(message) => formatter.write_str(message),
        }
    }
}

impl Error for LoadError {}

/// Computes the digest an embedder pins for an approved sleeve component.
#[must_use]
pub fn sleeve_sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Wires every plugin import to the matching sleeve export and verifies the result.
///
/// ```no_run
/// # fn load_plugin() -> Vec<u8> { Vec::new() }
/// # fn load_sleeve() -> Vec<u8> { Vec::new() }
/// let plugin = load_plugin();
/// let approved_sleeve = load_sleeve();
/// let pin = sleeve_host::sleeve_sha256(&approved_sleeve);
/// let component = sleeve_host::compose(&plugin, &approved_sleeve, pin)?;
/// # Ok::<(), sleeve_host::LoadError>(())
/// ```
///
/// # Errors
///
/// Returns [`LoadError`] when the digest is wrong, the sleeve cannot satisfy an
/// import, composition fails, or the encoded graph contains a direct bypass.
pub fn compose(
    plugin_bytes: &[u8],
    sleeve_bytes: &[u8],
    pinned_sha256: [u8; 32],
) -> Result<Vec<u8>, LoadError> {
    compose_with(
        plugin_bytes,
        sleeve_bytes,
        pinned_sha256,
        wire_plugin_to_sleeve,
    )
}

fn wire_plugin_to_sleeve(
    graph: &mut CompositionGraph,
    sleeve_instance: NodeId,
    plugin_instance: NodeId,
    plugin_imports: &[String],
) -> Result<(), LoadError> {
    for name in plugin_imports {
        let export = graph
            .alias_instance_export(sleeve_instance, name)
            .map_err(invalid)?;
        graph
            .set_instantiation_argument(plugin_instance, name, export)
            .map_err(invalid)?;
    }
    Ok(())
}

pub(crate) fn compose_with<W>(
    plugin_bytes: &[u8],
    sleeve_bytes: &[u8],
    pinned_sha256: [u8; 32],
    wire: W,
) -> Result<Vec<u8>, LoadError>
where
    W: FnOnce(&mut CompositionGraph, NodeId, NodeId, &[String]) -> Result<(), LoadError>,
{
    if sleeve_sha256(sleeve_bytes) != pinned_sha256 {
        return Err(LoadError::HashMismatch);
    }

    let composed = encode_composition(plugin_bytes, sleeve_bytes, wire)?;
    verify_composed_routing(&composed, plugin_bytes, sleeve_bytes)?;
    Ok(composed)
}

pub(crate) fn encode_composition<W>(
    plugin_bytes: &[u8],
    sleeve_bytes: &[u8],
    wire: W,
) -> Result<Vec<u8>, LoadError>
where
    W: FnOnce(&mut CompositionGraph, NodeId, NodeId, &[String]) -> Result<(), LoadError>,
{
    let mut graph = CompositionGraph::new();
    let sleeve = Package::from_bytes("sleeve:active", None, sleeve_bytes, graph.types_mut())
        .map_err(invalid)?;
    let plugin = Package::from_bytes("plugin:untrusted", None, plugin_bytes, graph.types_mut())
        .map_err(invalid)?;
    let plugin_imports = graph.types()[plugin.ty()]
        .imports
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let sleeve_exports = graph.types()[sleeve.ty()]
        .exports
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    for name in &plugin_imports {
        if !PLUGIN_INTERFACES.contains(&name.as_str()) {
            if name.starts_with("sleeve:platform/") || sleeve_exports.contains(name) {
                return Err(LoadError::HostFacingImport(name.clone()));
            }
            return Err(LoadError::UnsatisfiedImport(name.clone()));
        }
        if !sleeve_exports.contains(name) {
            return Err(LoadError::UnsatisfiedImport(name.clone()));
        }
    }
    let plugin_exports = graph.types()[plugin.ty()]
        .exports
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    if let Some(name) = plugin_exports
        .iter()
        .find(|name| name.starts_with("sleeve:platform/"))
    {
        return Err(LoadError::HostFacingExport(name.clone()));
    }
    let sleeve_host_exports = sleeve_exports
        .iter()
        .filter(|name| !PLUGIN_INTERFACES.contains(&name.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    let sleeve_id = graph.register_package(sleeve).map_err(invalid)?;
    let plugin_id = graph.register_package(plugin).map_err(invalid)?;
    let sleeve_instance = graph.instantiate(sleeve_id);
    let plugin_instance = graph.instantiate(plugin_id);
    wire(
        &mut graph,
        sleeve_instance,
        plugin_instance,
        &plugin_imports,
    )?;
    for name in plugin_exports {
        let export = graph
            .alias_instance_export(plugin_instance, &name)
            .map_err(invalid)?;
        graph.export(export, &name).map_err(invalid)?;
    }
    for name in sleeve_host_exports {
        let export = graph
            .alias_instance_export(sleeve_instance, &name)
            .map_err(invalid)?;
        graph.export(export, &name).map_err(invalid)?;
    }

    graph.encode(EncodeOptions::default()).map_err(invalid)
}

/// Verifies that an encoded composition routes the embedded plugin through aliases.
///
/// This check is separate from [`compose`] so callers and tests can validate an
/// already encoded graph rather than relying on graph-construction intent.
///
/// # Errors
///
/// Returns [`LoadError`] if the plugin cannot be identified, the component is
/// malformed, or a plugin import is not wired to the matching export of the
/// selected sleeve instance.
pub fn verify_composed_routing(
    bytes: &[u8],
    plugin_bytes: &[u8],
    sleeve_bytes: &[u8],
) -> Result<(), LoadError> {
    let mut graph = CompositionGraph::new();
    let plugin = Package::from_bytes("plugin:verify", None, plugin_bytes, graph.types_mut())
        .map_err(invalid)?;
    let plugin_imports = graph.types()[plugin.ty()]
        .imports
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    RoutingIndex::parse(bytes, plugin_bytes, sleeve_bytes)?.verify(&plugin_imports)
}

fn claim_index(next: &mut u32, space: &str) -> Result<u32, LoadError> {
    let index = *next;
    *next = next
        .checked_add(1)
        .ok_or_else(|| LoadError::InvalidComponent(format!("{space} index space overflowed")))?;
    Ok(index)
}

fn record_component(selected: &mut Option<u32>, index: u32, name: &str) -> Result<(), LoadError> {
    if selected.replace(index).is_some() {
        return Err(LoadError::InvalidComponent(format!(
            "composition embeds the {name} more than once"
        )));
    }
    Ok(())
}

fn invalid(error: impl Display) -> LoadError {
    LoadError::InvalidComponent(error.to_string())
}
