/// Implements the wrapped WASI filesystem exports inside a sleeve.
#[doc(hidden)]
#[macro_export]
macro_rules! impl_filesystem_wrapper {
    ($bindings:ident) => {
        const FILESYSTEM_TYPES: &str = "wasi:filesystem/types@0.3.0";
        const FILESYSTEM_PREOPENS: &str = "wasi:filesystem/preopens@0.3.0";
        const DESCRIPTOR_RESOURCE: &str = "wasi:filesystem/types.descriptor";
        const FILE_CHUNK_SIZE: usize = 64 * 1024;

        type ImportedDescriptor = $bindings::wasi::filesystem::types::Descriptor;
        type ExportedDescriptor = $bindings::exports::wasi::filesystem::types::Descriptor;
        type FileError = $crate::filesystem::ErrorCode;

        struct WrappedDescriptor {
            id: u64,
            inner: ::alloc::sync::Arc<ImportedDescriptor>,
        }

        impl WrappedDescriptor {
            fn new(id: u64, inner: ::alloc::sync::Arc<ImportedDescriptor>) -> Self {
                Self { id, inner }
            }
        }

        impl Drop for WrappedDescriptor {
            fn drop(&mut self) {
                observe_drop(self.id);
            }
        }

        static FILE_PREOPENS: ::spin::Mutex<
            ::core::option::Option<
                ::alloc::vec::Vec<(
                    ::alloc::sync::Arc<ImportedDescriptor>,
                    ::alloc::string::String,
                    ::alloc::string::String,
                )>,
            >,
        > = ::spin::Mutex::new(None);

        fn prepare_preopens() {
            let directories = $bindings::wasi::filesystem::preopens::get_directories()
                .into_iter()
                .map(|(descriptor, name)| {
                    let label = $bindings::sleeve::platform::settings::preopen_label(&name);
                    (::alloc::sync::Arc::new(descriptor), name, label)
                })
                .collect();
            if let Some(mut preopens) = FILE_PREOPENS.try_lock() {
                *preopens = Some(directories);
            } else {
                ::core::arch::wasm32::unreachable();
            }
        }

        struct FileChannel {
            id: ::core::option::Option<u64>,
        }

        impl FileChannel {
            fn close(&mut self) {
                if let Some(id) = self.id.take() {
                    close_channel(id);
                }
            }
        }

        impl Drop for FileChannel {
            fn drop(&mut self) {
                self.close();
            }
        }

        fn refused_file_future() -> ::wit_bindgen::FutureReader<Result<(), FileError>> {
            let (writer, reader) = $bindings::wit_future::new(|| Err(FileError::Access));
            dispatch_or_trap(SLEEVE.enqueue_relay(async move {
                let _written = writer.write(Err(FileError::Access)).await;
            }));
            reader
        }

        /// Relays a read stream without blocking the synchronous constructor.
        async fn relay_file_read(
            descriptor: ::alloc::sync::Arc<ImportedDescriptor>,
            offset: u64,
            mut destination: ::wit_bindgen::StreamWriter<u8>,
            result: ::wit_bindgen::FutureWriter<Result<(), FileError>>,
        ) {
            let (mut source, completion) = descriptor.read_via_stream(offset);
            copy_stream(&mut source, &mut destination).await;
            drop(destination);
            let completed = completion.await;
            let _written = result.write(completed).await;
        }

        /// The channel closes once the plugin writer is drained because the plugin
        /// can no longer send bytes, even if host completion is still pending.
        async fn relay_file_write(
            descriptor: ::alloc::sync::Arc<ImportedDescriptor>,
            mut source: ::wit_bindgen::StreamReader<u8>,
            offset: u64,
            mut destination: ::wit_bindgen::StreamWriter<u8>,
            host_reader: ::wit_bindgen::StreamReader<u8>,
            result: ::wit_bindgen::FutureWriter<Result<(), FileError>>,
            mut channel: FileChannel,
        ) {
            let completion = descriptor.write_via_stream(host_reader, offset);
            copy_stream(&mut source, &mut destination).await;
            drop(destination);
            channel.close();
            let completed = completion.await;
            let _written = result.write(completed).await;
        }

        async fn copy_stream(
            source: &mut ::wit_bindgen::StreamReader<u8>,
            destination: &mut ::wit_bindgen::StreamWriter<u8>,
        ) {
            loop {
                let (status, bytes) = source
                    .read(::alloc::vec::Vec::with_capacity(FILE_CHUNK_SIZE))
                    .await;
                if !bytes.is_empty() && !destination.write_all(bytes).await.is_empty() {
                    break;
                }
                if !matches!(status, ::wit_bindgen::StreamResult::Complete(_)) {
                    break;
                }
            }
        }

        impl $bindings::exports::wasi::filesystem::types::Guest for Component {
            type Descriptor = WrappedDescriptor;
        }

        impl $bindings::exports::wasi::filesystem::types::GuestDescriptor for WrappedDescriptor {
            fn read_via_stream(
                &self,
                offset: u64,
            ) -> (
                ::wit_bindgen::StreamReader<u8>,
                ::wit_bindgen::FutureReader<Result<(), FileError>>,
            ) {
                let stream_id = SLEEVE.next_handle();
                let future_id = SLEEVE.next_handle();
                dispatch_or_trap(SLEEVE.dispatch_sync(
                    FILESYSTEM_TYPES,
                    "[method]descriptor.read-via-stream",
                    ::alloc::vec![$crate::Designator::new(
                        "offset",
                        ::alloc::format!("{offset}")
                    )],
                    ::alloc::vec![self.id],
                    ::alloc::vec![
                        (stream_id, "stream<u8>"),
                        (future_id, "future<filesystem-result>")
                    ],
                    |_| {
                        let (destination, reader) = $bindings::wit_stream::new();
                        let (result, result_reader) =
                            $bindings::wit_future::new(|| Err(FileError::Access));
                        dispatch_or_trap(SLEEVE.enqueue_relay(relay_file_read(
                            ::alloc::sync::Arc::clone(&self.inner),
                            offset,
                            destination,
                            result,
                        )));
                        (reader, result_reader)
                    },
                ))
            }

            fn write_via_stream(
                &self,
                data: ::wit_bindgen::StreamReader<u8>,
                offset: u64,
            ) -> ::wit_bindgen::FutureReader<Result<(), FileError>> {
                let channel_id = SLEEVE.next_handle();
                let descriptor_id = self.id;
                let dispatched = SLEEVE.dispatch_sync(
                    FILESYSTEM_TYPES,
                    "[method]descriptor.write-via-stream",
                    ::alloc::vec![$crate::Designator::new(
                        "offset",
                        ::alloc::format!("{offset}")
                    )],
                    ::alloc::vec![descriptor_id],
                    ::alloc::vec::Vec::new(),
                    |call_id| {
                        SLEEVE.open_channel_to(
                            channel_id,
                            $crate::ChannelKind::Stream,
                            call_id,
                            descriptor_id,
                        )?;
                        let (destination, reader) = $bindings::wit_stream::new();
                        let (result, result_reader) =
                            $bindings::wit_future::new(|| Err(FileError::Access));
                        let relay = relay_file_write(
                            ::alloc::sync::Arc::clone(&self.inner),
                            data,
                            offset,
                            destination,
                            reader,
                            result,
                            FileChannel {
                                id: Some(channel_id),
                            },
                        );
                        SLEEVE.enqueue_relay(relay)?;
                        Ok::<_, $crate::DispatchError>(result_reader)
                    },
                );
                match dispatched {
                    Ok(Ok(result)) => result,
                    Ok(Err($crate::DispatchError::Denied(_)))
                    | Err($crate::DispatchError::Denied(_)) => refused_file_future(),
                    Ok(Err(error)) | Err(error) => error.trap(),
                }
            }

            async fn stat(&self) -> Result<$crate::filesystem::DescriptorStat, FileError> {
                $crate::dispatch_domain(
                    SLEEVE
                        .dispatch_result(
                            FILESYSTEM_TYPES,
                            "[method]descriptor.stat",
                            ::alloc::vec::Vec::new(),
                            ::alloc::vec![self.id],
                            ::alloc::vec::Vec::new(),
                            |_| self.inner.stat(),
                        )
                        .await,
                    FileError::Access,
                )
            }

            async fn open_at(
                &self,
                path_flags: $crate::filesystem::PathFlags,
                path: ::alloc::string::String,
                open_flags: $crate::filesystem::OpenFlags,
                flags: $crate::filesystem::DescriptorFlags,
            ) -> Result<ExportedDescriptor, FileError> {
                let id = SLEEVE.next_handle();
                let writing = flags.contains($crate::filesystem::DescriptorFlags::WRITE)
                    || open_flags.contains($crate::filesystem::OpenFlags::CREATE)
                    || open_flags.contains($crate::filesystem::OpenFlags::TRUNCATE);
                let result = $crate::dispatch_domain(
                    SLEEVE
                        .dispatch_result_derived(
                            FILESYSTEM_TYPES,
                            "[method]descriptor.open-at",
                            ::alloc::vec![
                                $crate::Designator::new("path", path.as_str()),
                                $crate::Designator::new(
                                    "write",
                                    if writing { "true" } else { "false" },
                                ),
                            ],
                            ::alloc::vec![self.id],
                            (id, DESCRIPTOR_RESOURCE, self.id),
                            |_| async {
                                $crate::filesystem::validate_relative_path(&path)?;
                                self.inner
                                    .open_at(path_flags, path.clone(), open_flags, flags)
                                    .await
                            },
                        )
                        .await,
                    FileError::Access,
                )?;
                Ok(ExportedDescriptor::new(WrappedDescriptor::new(
                    id,
                    ::alloc::sync::Arc::new(result),
                )))
            }
        }

        impl $bindings::exports::wasi::filesystem::preopens::Guest for Component {
            fn get_directories() -> ::alloc::vec::Vec<(ExportedDescriptor, ::alloc::string::String)>
            {
                let directories = match FILE_PREOPENS.try_lock() {
                    Some(preopens) => preopens.as_ref().cloned().unwrap_or_default(),
                    None => ::core::arch::wasm32::unreachable(),
                };
                let mut designators = ::alloc::vec::Vec::with_capacity(directories.len() * 2);
                for (_, name, label) in &directories {
                    designators.push($crate::Designator::new("name", name.clone()));
                    designators.push($crate::Designator::new("label", label.clone()));
                }
                dispatch_or_trap(SLEEVE.dispatch_sync_handles(
                    FILESYSTEM_PREOPENS,
                    "get-directories",
                    designators,
                    ::alloc::vec::Vec::new(),
                    |call_id| {
                        let mut produced = ::alloc::vec::Vec::with_capacity(directories.len());
                        let wrapped = directories
                            .into_iter()
                            .map(|(descriptor, name, _)| {
                                let id = SLEEVE.next_handle();
                                produced.push($crate::ProducedHandle::from_call(
                                    id,
                                    DESCRIPTOR_RESOURCE,
                                    call_id,
                                ));
                                (
                                    ExportedDescriptor::new(WrappedDescriptor::new(id, descriptor)),
                                    name,
                                )
                            })
                            .collect();
                        (wrapped, produced)
                    },
                ))
            }
        }
    };
}

/// Implements a filesystem sleeve with the supplied policy chain.
#[macro_export]
macro_rules! export_file_sleeve {
    ($bindings:ident, $chain:expr) => {
        static SLEEVE: $crate::Sleeve = $crate::Sleeve::new();

        struct Component;

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

        $crate::impl_filesystem_wrapper!($bindings);

        impl $bindings::exports::sleeve::platform::lifecycle::Guest for Component {
            fn start(invocation: ::alloc::string::String) {
                dispatch_or_trap(SLEEVE.start($chain, invocation));
                prepare_preopens();
            }

            fn end(invocation: ::alloc::string::String, trapped: bool) {
                dispatch_or_trap(SLEEVE.end(invocation, trapped));
            }
        }

        $crate::export_anchor!($bindings);

        #[allow(unsafe_code)]
        mod component_export {
            use super::{Component, bindings};
            bindings::export!(Component with_types_in bindings);
        }
    };
}
