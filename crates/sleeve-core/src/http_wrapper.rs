/// Implements the wrapped notes and WASI HTTP exports for a sleeve variant.
#[macro_export]
macro_rules! export_http_sleeve {
    ($bindings:ident, $chain:expr) => {
        static SLEEVE: $crate::Sleeve = $crate::Sleeve::new();

        const HTTP_TYPES: &str = "wasi:http/types@0.3.0";
        const HTTP_CLIENT: &str = "wasi:http/client@0.3.0";

        type ImportedFields = $bindings::wasi::http::types::Fields;
        type ImportedOptions = $bindings::wasi::http::types::RequestOptions;
        type ImportedRequest = $bindings::wasi::http::types::Request;
        type ImportedResponse = $bindings::wasi::http::types::Response;
        type ExportedFields = $bindings::exports::wasi::http::types::Fields;
        type ExportedOptions = $bindings::exports::wasi::http::types::RequestOptions;
        type ExportedRequest = $bindings::exports::wasi::http::types::Request;
        type ExportedResponse = $bindings::exports::wasi::http::types::Response;
        type ErrorCode = $crate::http::ErrorCode;

        struct Component;

        struct Wrapped<T> {
            id: u64,
            inner: ::core::option::Option<T>,
        }

        impl<T> Wrapped<T> {
            fn new(id: u64, inner: T) -> Self {
                Self {
                    id,
                    inner: Some(inner),
                }
            }

            fn inner(&self) -> &T {
                match &self.inner {
                    Some(inner) => inner,
                    None => ::core::arch::wasm32::unreachable(),
                }
            }

            fn take(mut self) -> T {
                observe_drop(self.id);
                match self.inner.take() {
                    Some(inner) => inner,
                    None => ::core::arch::wasm32::unreachable(),
                }
            }
        }

        impl<T> Drop for Wrapped<T> {
            fn drop(&mut self) {
                if self.inner.is_some() {
                    observe_drop(self.id);
                }
            }
        }

        type WrappedFields = Wrapped<ImportedFields>;
        type WrappedOptions = Wrapped<ImportedOptions>;
        type WrappedResponse = Wrapped<ImportedResponse>;

        impl $bindings::exports::wasi::http::types::GuestFields for WrappedFields {
            fn new() -> Self {
                let id = SLEEVE.next_handle();
                dispatch_or_trap(SLEEVE.dispatch_sync(
                    HTTP_TYPES,
                    "[constructor]fields",
                    ::alloc::vec::Vec::new(),
                    ::alloc::vec::Vec::new(),
                    ::alloc::vec![(id, "wasi:http/types.fields")],
                    |_| Self::new(id, ImportedFields::new()),
                ))
            }

            fn from_list(
                entries: ::alloc::vec::Vec<(::alloc::string::String, ::alloc::vec::Vec<u8>)>,
            ) -> Result<ExportedFields, $bindings::exports::wasi::http::types::HeaderError> {
                let id = SLEEVE.next_handle();
                dispatch_or_trap(SLEEVE.dispatch_sync_result(
                    HTTP_TYPES,
                    "[static]fields.from-list",
                    ::alloc::vec::Vec::new(),
                    ::alloc::vec::Vec::new(),
                    ::alloc::vec![(id, "wasi:http/types.fields")],
                    |_| {
                        ImportedFields::from_list(&entries)
                            .map(|fields| ExportedFields::new(Self::new(id, fields)))
                    },
                ))
                .map_err(map_header_error)
            }

            fn append(
                &self,
                name: ::alloc::string::String,
                value: ::alloc::vec::Vec<u8>,
            ) -> Result<(), $bindings::exports::wasi::http::types::HeaderError> {
                dispatch_or_trap(SLEEVE.dispatch_sync_result(
                    HTTP_TYPES,
                    "[method]fields.append",
                    ::alloc::vec![$crate::Designator::new("name", name.as_str())],
                    ::alloc::vec![self.id],
                    ::alloc::vec::Vec::new(),
                    |_| self.inner().append(&name, &value),
                ))
                .map_err(map_header_error)
            }

            fn copy_all(
                &self,
            ) -> ::alloc::vec::Vec<(::alloc::string::String, ::alloc::vec::Vec<u8>)> {
                dispatch_or_trap(SLEEVE.dispatch_sync(
                    HTTP_TYPES,
                    "[method]fields.copy-all",
                    ::alloc::vec::Vec::new(),
                    ::alloc::vec![self.id],
                    ::alloc::vec::Vec::new(),
                    |_| self.inner().copy_all(),
                ))
            }
        }

        impl $bindings::exports::wasi::http::types::GuestRequestOptions for WrappedOptions {
            fn new() -> Self {
                let id = SLEEVE.next_handle();
                dispatch_or_trap(SLEEVE.dispatch_sync(
                    HTTP_TYPES,
                    "[constructor]request-options",
                    ::alloc::vec::Vec::new(),
                    ::alloc::vec::Vec::new(),
                    ::alloc::vec![(id, "wasi:http/types.request-options")],
                    |_| Self::new(id, ImportedOptions::new()),
                ))
            }
        }
    };
}
