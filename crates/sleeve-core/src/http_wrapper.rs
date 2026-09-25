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

        struct RequestChannels {
            body: ::core::option::Option<u64>,
            trailers: ::core::option::Option<u64>,
        }

        impl RequestChannels {
            fn close_body(&mut self) {
                if let Some(id) = self.body.take() {
                    close_channel(id);
                }
            }

            fn close_trailers(&mut self) {
                if let Some(id) = self.trailers.take() {
                    close_channel(id);
                }
            }

            fn close_all(&mut self) {
                self.close_body();
                self.close_trailers();
            }
        }

        impl Drop for RequestChannels {
            fn drop(&mut self) {
                self.close_all();
            }
        }

        struct RequestParts {
            headers: ImportedFields,
            body: ::core::option::Option<::wit_bindgen::StreamReader<u8>>,
            trailers: ::wit_bindgen::FutureReader<
                Result<::core::option::Option<ExportedFields>, ErrorCode>,
            >,
            options: ::core::option::Option<ImportedOptions>,
            method: $bindings::exports::wasi::http::types::Method,
            path: ::core::option::Option<::alloc::string::String>,
            scheme: ::core::option::Option<$bindings::exports::wasi::http::types::Scheme>,
            authority: ::core::option::Option<::alloc::string::String>,
            channels: RequestChannels,
            result_id: u64,
            result_writer: ::wit_bindgen::FutureWriter<Result<(), ErrorCode>>,
        }

        struct WrappedRequest {
            id: u64,
            parts: ::core::cell::RefCell<::core::option::Option<RequestParts>>,
        }

        impl WrappedRequest {
            fn take(mut self) -> RequestParts {
                observe_drop(self.id);
                match self.parts.get_mut().take() {
                    Some(parts) => parts,
                    None => ::core::arch::wasm32::unreachable(),
                }
            }

            fn update(&self, update: impl FnOnce(&mut RequestParts)) -> Result<(), ()> {
                let mut parts = self.parts.try_borrow_mut().map_err(|_| ())?;
                let parts = parts.as_mut().ok_or(())?;
                update(parts);
                Ok(())
            }
        }

        impl Drop for WrappedRequest {
            fn drop(&mut self) {
                if let Some(parts) = self.parts.get_mut().take() {
                    observe_drop(parts.result_id);
                    observe_drop(self.id);
                }
            }
        }

        fn observe_drop(id: u64) {
            let _result = SLEEVE.drop_handle(id);
        }

        fn close_channel(id: u64) {
            if let Err(error) = SLEEVE.close_channel(id) {
                error.trap();
            }
        }

        fn dispatch_or_trap<T>(result: Result<T, $crate::DispatchError>) -> T {
            match result {
                Ok(value) => value,
                Err(error) => error.trap(),
            }
        }

        fn http_dispatch(
            result: Result<Result<ImportedResponse, ErrorCode>, $crate::DispatchError>,
        ) -> Result<ImportedResponse, ErrorCode> {
            match result {
                Ok(result) => result,
                Err(error) => match error.into_http_denial() {
                    Ok(error) => Err(error),
                    Err(error) => error.trap(),
                },
            }
        }

        fn map_header_error(
            error: $bindings::wasi::http::types::HeaderError,
        ) -> $bindings::exports::wasi::http::types::HeaderError {
            use $bindings::exports::wasi::http::types::HeaderError as Out;
            use $bindings::wasi::http::types::HeaderError as In;
            match error {
                In::InvalidSyntax => Out::InvalidSyntax,
                In::Forbidden => Out::Forbidden,
                In::Immutable => Out::Immutable,
                In::SizeExceeded => Out::SizeExceeded,
                In::Other(message) => Out::Other(message),
            }
        }

        fn import_method(
            method: $bindings::exports::wasi::http::types::Method,
        ) -> $bindings::wasi::http::types::Method {
            use $bindings::exports::wasi::http::types::Method as Out;
            use $bindings::wasi::http::types::Method as In;
            match method {
                Out::Get => In::Get,
                Out::Head => In::Head,
                Out::Post => In::Post,
                Out::Put => In::Put,
                Out::Delete => In::Delete,
                Out::Connect => In::Connect,
                Out::Options => In::Options,
                Out::Trace => In::Trace,
                Out::Patch => In::Patch,
                Out::Other(method) => In::Other(method),
            }
        }

        fn import_scheme(
            scheme: &$bindings::exports::wasi::http::types::Scheme,
        ) -> $bindings::wasi::http::types::Scheme {
            use $bindings::exports::wasi::http::types::Scheme as Out;
            use $bindings::wasi::http::types::Scheme as In;
            match scheme {
                Out::Http => In::Http,
                Out::Https => In::Https,
                Out::Other(scheme) => In::Other(scheme.clone()),
            }
        }

        fn scheme_name(
            scheme: &$bindings::exports::wasi::http::types::Scheme,
        ) -> &str {
            use $bindings::exports::wasi::http::types::Scheme;
            match scheme {
                Scheme::Http => "http",
                Scheme::Https => "https",
                Scheme::Other(scheme) => scheme,
            }
        }

        fn origin(parts: &RequestParts) -> Result<::alloc::string::String, ErrorCode> {
            let scheme = parts
                .scheme
                .as_ref()
                .ok_or(ErrorCode::HttpRequestUriInvalid)?;
            let authority = parts
                .authority
                .as_deref()
                .ok_or(ErrorCode::HttpRequestUriInvalid)?;
            $crate::http::normalize_origin(scheme_name(scheme), authority)
                .map_err(|_| ErrorCode::HttpRequestUriInvalid)
        }

        async fn buffer_request(
            mut parts: RequestParts,
        ) -> Result<
            (
                ImportedRequest,
                ::wit_bindgen::FutureReader<Result<(), ErrorCode>>,
                ::wit_bindgen::FutureWriter<Result<(), ErrorCode>>,
                u64,
            ),
            ErrorCode,
        > {
            let limit = $bindings::sleeve::platform::settings::request_body_limit();
            let had_body = parts.body.is_some();
            let mut body = ::alloc::vec::Vec::new();
            if let Some(mut stream) = parts.body.take() {
                while let Some(byte) = stream.next().await {
                    let size = u64::try_from(body.len())
                        .unwrap_or(u64::MAX)
                        .saturating_add(1);
                    if size > limit {
                        let writer = parts.result_writer;
                        let result_id = parts.result_id;
                        ::wit_bindgen::spawn_local(async move {
                            let _write = writer
                                .write(Err(ErrorCode::HttpRequestBodySize(Some(limit))))
                                .await;
                            observe_drop(result_id);
                        });
                        return Err(ErrorCode::HttpRequestBodySize(Some(limit)));
                    }
                    body.push(byte);
                }
                parts.channels.close_body();
            }
            let trailers = parts.trailers.await?;
            parts.channels.close_trailers();
            let trailers = trailers.map(|fields| fields.into_inner::<WrappedFields>().take());

            let body = if had_body {
                let (mut writer, reader) = $bindings::wit_stream::new();
                ::wit_bindgen::spawn_local(async move {
                    let _remaining = writer.write_all(body).await;
                    drop(writer);
                });
                Some(reader)
            } else {
                None
            };
            let (trailers_writer, trailers_reader) =
                $bindings::wit_future::new(|| Ok(None));
            if let Some(trailers) = trailers {
                ::wit_bindgen::spawn_local(async move {
                    let _write = trailers_writer.write(Ok(Some(trailers))).await;
                });
            } else {
                drop(trailers_writer);
            }
            let (request, result) = $bindings::wasi::http::types::Request::new(
                parts.headers,
                body,
                trailers_reader,
                parts.options,
            );
            request
                .set_method(&import_method(parts.method))
                .map_err(|()| ErrorCode::HttpRequestMethodInvalid)?;
            request
                .set_path_with_query(parts.path.as_deref())
                .map_err(|()| ErrorCode::HttpRequestUriInvalid)?;
            request
                .set_scheme(parts.scheme.as_ref().map(import_scheme).as_ref())
                .map_err(|()| ErrorCode::HttpRequestUriInvalid)?;
            request
                .set_authority(parts.authority.as_deref())
                .map_err(|()| ErrorCode::HttpRequestUriInvalid)?;
            Ok((request, result, parts.result_writer, parts.result_id))
        }

        async fn forward_request(parts: RequestParts) -> Result<ImportedResponse, ErrorCode> {
            let (request, transmission, result_writer, result_id) = buffer_request(parts).await?;
            ::wit_bindgen::spawn_local(async move {
                let result = transmission.await;
                let _write = result_writer.write(result).await;
                observe_drop(result_id);
            });
            $bindings::wasi::http::client::send(request).await
        }

        impl $bindings::exports::example::notes::notes::Guest for Component {
            async fn read(name: ::alloc::string::String) -> ::alloc::string::String {
                dispatch_or_trap(
                    SLEEVE
                        .dispatch(
                            "example:notes/notes@0.1.0",
                            "read",
                            ::alloc::vec![$crate::Designator::new("name", name.as_str())],
                            $bindings::example::notes::notes::read(name.clone()),
                        )
                        .await,
                )
            }
        }

        impl $bindings::exports::wasi::http::types::Guest for Component {
            type Fields = WrappedFields;
            type RequestOptions = WrappedOptions;
            type Request = WrappedRequest;
            type Response = WrappedResponse;
        }

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

        impl $bindings::exports::wasi::http::types::GuestRequest for WrappedRequest {
            fn new(
                headers: ExportedFields,
                contents: ::core::option::Option<::wit_bindgen::StreamReader<u8>>,
                trailers: ::wit_bindgen::FutureReader<
                    Result<::core::option::Option<ExportedFields>, ErrorCode>,
                >,
                options: ::core::option::Option<ExportedOptions>,
            ) -> (
                ExportedRequest,
                ::wit_bindgen::FutureReader<Result<(), ErrorCode>>,
            ) {
                let headers_id = headers.get::<WrappedFields>().id;
                let headers = headers.into_inner::<WrappedFields>().take();
                let (options, option_id) = match options {
                    Some(options) => {
                        let id = options.get::<WrappedOptions>().id;
                        (Some(options.into_inner::<WrappedOptions>().take()), Some(id))
                    }
                    None => (None, None),
                };
                let id = SLEEVE.next_handle();
                let result_id = SLEEVE.next_handle();
                let body_channel = contents.as_ref().map(|_| SLEEVE.next_handle());
                let trailers_channel = SLEEVE.next_handle();
                let (result_writer, result_reader) =
                    $bindings::wit_future::new(|| Ok(()));
                let mut handles = ::alloc::vec![headers_id];
                if let Some(id) = option_id {
                    handles.push(id);
                }
                let request = dispatch_or_trap(SLEEVE.dispatch_sync(
                    HTTP_TYPES,
                    "[static]request.new",
                    ::alloc::vec::Vec::new(),
                    handles,
                    ::alloc::vec![
                        (id, "wasi:http/types.request"),
                        (result_id, "future<wasi:http/request-result>"),
                    ],
                    |call_id| {
                        if let Some(channel) = body_channel {
                            dispatch_or_trap(SLEEVE.open_channel(
                                channel,
                                $crate::ChannelKind::Stream,
                                call_id,
                            ));
                        }
                        dispatch_or_trap(SLEEVE.open_channel(
                            trailers_channel,
                            $crate::ChannelKind::Future,
                            call_id,
                        ));
                        ExportedRequest::new(WrappedRequest {
                            id,
                            parts: ::core::cell::RefCell::new(Some(RequestParts {
                                headers,
                                body: contents,
                                trailers,
                                options,
                                method: $bindings::exports::wasi::http::types::Method::Get,
                                path: None,
                                scheme: None,
                                authority: None,
                                channels: RequestChannels {
                                    body: body_channel,
                                    trailers: Some(trailers_channel),
                                },
                                result_id,
                                result_writer,
                            })),
                        })
                    },
                ));
                (request, result_reader)
            }

            fn set_method(
                &self,
                method: $bindings::exports::wasi::http::types::Method,
            ) -> Result<(), ()> {
                dispatch_or_trap(SLEEVE.dispatch_sync_result(
                    HTTP_TYPES,
                    "[method]request.set-method",
                    ::alloc::vec::Vec::new(),
                    ::alloc::vec![self.id],
                    ::alloc::vec::Vec::new(),
                    |_| self.update(|parts| parts.method = method),
                ))
            }

            fn set_path_with_query(
                &self,
                path: ::core::option::Option<::alloc::string::String>,
            ) -> Result<(), ()> {
                dispatch_or_trap(SLEEVE.dispatch_sync_result(
                    HTTP_TYPES,
                    "[method]request.set-path-with-query",
                    ::alloc::vec::Vec::new(),
                    ::alloc::vec![self.id],
                    ::alloc::vec::Vec::new(),
                    |_| self.update(|parts| parts.path = path),
                ))
            }

            fn set_scheme(
                &self,
                scheme: ::core::option::Option<$bindings::exports::wasi::http::types::Scheme>,
            ) -> Result<(), ()> {
                dispatch_or_trap(SLEEVE.dispatch_sync_result(
                    HTTP_TYPES,
                    "[method]request.set-scheme",
                    ::alloc::vec::Vec::new(),
                    ::alloc::vec![self.id],
                    ::alloc::vec::Vec::new(),
                    |_| self.update(|parts| parts.scheme = scheme),
                ))
            }

            fn set_authority(
                &self,
                authority: ::core::option::Option<::alloc::string::String>,
            ) -> Result<(), ()> {
                dispatch_or_trap(SLEEVE.dispatch_sync_result(
                    HTTP_TYPES,
                    "[method]request.set-authority",
                    ::alloc::vec::Vec::new(),
                    ::alloc::vec![self.id],
                    ::alloc::vec::Vec::new(),
                    |_| self.update(|parts| parts.authority = authority),
                ))
            }
        }

        impl $bindings::exports::wasi::http::types::GuestResponse for WrappedResponse {
            fn get_status_code(&self) -> u16 {
                dispatch_or_trap(SLEEVE.dispatch_sync(
                    HTTP_TYPES,
                    "[method]response.get-status-code",
                    ::alloc::vec::Vec::new(),
                    ::alloc::vec![self.id],
                    ::alloc::vec::Vec::new(),
                    |_| self.inner().get_status_code(),
                ))
            }

            fn consume_body(
                response: ExportedResponse,
                result: ::wit_bindgen::FutureReader<Result<(), ErrorCode>>,
            ) -> (
                ::wit_bindgen::StreamReader<u8>,
                ::wit_bindgen::FutureReader<
                    Result<::core::option::Option<ExportedFields>, ErrorCode>,
                >,
            ) {
                let response = response.into_inner::<WrappedResponse>();
                let response_id = response.id;
                let response = response.take();
                let stream_id = SLEEVE.next_handle();
                let trailers_id = SLEEVE.next_handle();
                dispatch_or_trap(SLEEVE.dispatch_sync(
                    HTTP_TYPES,
                    "[static]response.consume-body",
                    ::alloc::vec::Vec::new(),
                    ::alloc::vec![response_id],
                    ::alloc::vec![
                        (stream_id, "stream<u8>"),
                        (trailers_id, "future<wasi:http/response-trailers>"),
                    ],
                    |_| {
                        let (result_writer, result_reader) =
                            $bindings::wit_future::new(|| Ok(()));
                        ::wit_bindgen::spawn_local(async move {
                            let value = result.await;
                            let _write = result_writer.write(value).await;
                        });
                        let (mut body, trailers) =
                            ImportedResponse::consume_body(response, result_reader);
                        let (mut body_writer, body_reader) = $bindings::wit_stream::new();
                        let (trailers_writer, trailers_reader) =
                            $bindings::wit_future::new(|| Ok(None));
                        ::wit_bindgen::spawn_local(async move {
                            while let Some(byte) = body.next().await {
                                if body_writer.write_one(byte).await.is_some() {
                                    break;
                                }
                            }
                            drop(body_writer);
                            observe_drop(stream_id);
                            let trailers = trailers.await.map(|trailers| {
                                trailers.map(|fields| {
                                    let id = SLEEVE.next_handle();
                                    dispatch_or_trap(SLEEVE.register_handle(
                                        $crate::ProducedHandle::from_parent(
                                            id,
                                            "wasi:http/types.fields",
                                            response_id,
                                        ),
                                    ));
                                    ExportedFields::new(WrappedFields::new(id, fields))
                                })
                            });
                            let _write = trailers_writer.write(trailers).await;
                            observe_drop(trailers_id);
                        });
                        (body_reader, trailers_reader)
                    },
                ))
            }
        }

        impl $bindings::exports::wasi::http::client::Guest for Component {
            async fn send(request: ExportedRequest) -> Result<ExportedResponse, ErrorCode> {
                let request_id = request.get::<WrappedRequest>().id;
                let request = request.into_inner::<WrappedRequest>().take();
                let origin = origin(&request)?;
                let response_id = SLEEVE.next_handle();
                let response = http_dispatch(
                    SLEEVE
                        .dispatch_result(
                            HTTP_CLIENT,
                            "send",
                            ::alloc::vec![$crate::Designator::new("origin", origin)],
                            ::alloc::vec![request_id],
                            ::alloc::vec![(response_id, "wasi:http/types.response")],
                            |_| forward_request(request),
                        )
                        .await,
                )?;
                Ok(ExportedResponse::new(WrappedResponse::new(
                    response_id,
                    response,
                )))
            }
        }

        impl $bindings::exports::sleeve::platform::lifecycle::Guest for Component {
            fn start(invocation: ::alloc::string::String) {
                dispatch_or_trap(SLEEVE.start($chain, invocation));
            }

            fn end(invocation: ::alloc::string::String, trapped: bool) {
                dispatch_or_trap(SLEEVE.end(invocation, trapped));
            }
        }

        #[allow(unsafe_code)]
        mod component_export {
            use super::{bindings, Component};
            bindings::export!(Component with_types_in bindings);
        }
    };
}
