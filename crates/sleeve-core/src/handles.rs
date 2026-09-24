use alloc::{boxed::Box, vec::Vec};
use core::any::{Any, TypeId};

use crate::{Metadata, ProducedHandle, Provenance};

struct Entry {
    handle: ProducedHandle,
    metadata: Vec<(TypeId, Box<dyn Any + Send>)>,
}

/// Stores wrapped handles, their provenance, and policy-private metadata.
#[derive(Default)]
pub struct HandleTable {
    entries: Vec<Entry>,
}

impl HandleTable {
    /// Creates an empty handle table without allocating.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Records a newly produced handle.
    pub fn insert(&mut self, handle: ProducedHandle) {
        self.entries.push(Entry {
            handle,
            metadata: Vec::new(),
        });
    }

    /// Returns the provenance for a recorded handle.
    #[must_use]
    pub fn provenance(&self, id: u64) -> Option<Provenance> {
        self.entries
            .iter()
            .find(|entry| entry.handle.id == id)
            .map(|entry| entry.handle.provenance)
    }

    /// Returns typed metadata previously attached by a policy.
    #[must_use]
    pub fn metadata<T: Any + Send>(&self, id: u64) -> Option<&T> {
        self.entries
            .iter()
            .find(|entry| entry.handle.id == id)?
            .metadata
            .iter()
            .find(|(kind, _)| *kind == TypeId::of::<T>())?
            .1
            .downcast_ref()
    }

    pub(crate) fn attach(&mut self, metadata: Metadata) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.handle.id == metadata.handle)
        {
            let kind = metadata.value.as_ref().type_id();
            if let Some((_, value)) = entry
                .metadata
                .iter_mut()
                .find(|(existing, _)| *existing == kind)
            {
                *value = metadata.value;
            } else {
                entry.metadata.push((kind, metadata.value));
            }
        }
    }
}
