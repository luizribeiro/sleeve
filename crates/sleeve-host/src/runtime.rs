use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};

use wasmtime::component::{
    Access, Accessor, Component, FutureConsumer, FutureProducer, FutureReader, HasData, Linker,
    Resource, ResourceTable, Source, StreamReader,
};
use wasmtime::{AsContextMut, Config, Engine, Store, StoreContextMut};
use wasmtime_wasi::filesystem::{Descriptor, WasiFilesystem, WasiFilesystemView as _};
use wasmtime_wasi::p3::bindings::filesystem::{preopens, types as wasi_types};
use wasmtime_wasi::p3::filesystem::FilesystemError;
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpCtxView, WasiHttpView};

use crate::compose;

mod bindings {
    wasmtime::component::bindgen!({
        path: ["../../wit/notes", "../../wit/http", "../../wit/clocks", "../../wit/filesystem", "../../wit/platform", "wit"],
        world: "sleeve:host/composed@0.1.0",
        imports: {
            "example:notes/notes@0.1.0": async | store,
            default: trappable,
        },
        exports: { default: async | store },
        require_store_data_send: true,
    });
}

mod http_bindings {
    wasmtime::component::bindgen!({
        path: ["../../wit/notes", "../../wit/http", "../../wit/clocks", "../../wit/filesystem", "../../wit/platform", "wit"],
        world: "sleeve:host/http-composed@0.1.0",
        imports: {
            "example:notes/notes@0.1.0": async | store,
            default: trappable,
        },
        exports: { default: async | store },
        require_store_data_send: true,
    });
}

mod file_bindings {
    wasmtime::component::bindgen!({
        path: ["../../wit/notes", "../../wit/http", "../../wit/clocks", "../../wit/filesystem", "../../wit/platform", "wit"],
        world: "sleeve:host/file-composed@0.1.0",
        imports: {
            "wasi:filesystem/types@0.3.0": store | trappable,
            default: trappable,
        },
        exports: { default: async | store },
        with: {
            "wasi:filesystem/types.descriptor": wasmtime_wasi::filesystem::Descriptor,
        },
        require_store_data_send: true,
    });
}

/// Owns immutable host services used to create one store per invocation.
pub struct Host {
    engine: Engine,
    notes: BTreeMap<String, String>,
    sleeve_sha256: [u8; 32],
}

/// Runs HTTP-capable plugins with per-invocation policy settings.
pub struct HttpHost {
    engine: Engine,
    notes: BTreeMap<String, String>,
    sleeve_sha256: [u8; 32],
    request_body_limit: u64,
    allowed_origins: Vec<String>,
}

/// Runs filesystem-capable plugins against labeled preopened directories.
pub struct FileHost {
    engine: Engine,
    sleeve_sha256: [u8; 32],
    preopens: Vec<FilePreopen>,
    hold_file_completions: bool,
}

/// One host directory exposed to a plugin under an opaque policy label.
pub struct FilePreopen {
    path: PathBuf,
    name: String,
    label: String,
}

impl FilePreopen {
    /// Describes a read-write preopen and its policy label.
    pub fn new(path: impl AsRef<Path>, name: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            path: path.as_ref().to_owned(),
            name: name.into(),
            label: label.into(),
        }
    }
}

/// The plugin's return value and the audit records persisted during its run.
#[derive(Debug, Eq, PartialEq)]
pub struct InvocationResult {
    /// Value returned by the plugin export.
    pub value: String,
    /// Records in the order the platform received them.
    pub audit: Vec<String>,
}

/// A completed or trapped export together with audit records already persisted.
#[derive(Debug, Eq, PartialEq)]
pub struct InvocationAttempt {
    /// The plugin value or trap diagnostic.
    pub value: Result<String, String>,
    /// Records in the order the platform received them.
    pub audit: Vec<String>,
}

struct State {
    notes: BTreeMap<String, String>,
    audit: Vec<String>,
    request_body_limit: u64,
    allowed_origins: Vec<String>,
    preopen_labels: BTreeMap<String, String>,
    table: ResourceTable,
    http: WasiHttpCtx,
    wasi: WasiCtx,
    hold_file_completions: bool,
}

struct StateView<'a>(&'a mut State);

struct StateData;

macro_rules! run_anchored {
    ($store:expr, $guest:expr, |$accessor:ident| $call:expr) => {
        $store
            .run_concurrent(async |$accessor| {
                let anchor = $guest.sleeve_platform_anchor().call_run($accessor);
                let invocation = async {
                    let value = $call.await;
                    let stopped = $guest.sleeve_platform_anchor().call_stop($accessor).await;
                    match value {
                        Err(error) => Err(error),
                        Ok(value) => {
                            stopped?;
                            Ok(value)
                        }
                    }
                };
                let (anchor, value) = tokio::join!(anchor, invocation);
                anchor?;
                value
            })
            .await
    };
}

impl HasData for StateData {
    type Data<'a> = StateView<'a>;
}

impl bindings::example::notes::notes::Host for StateView<'_> {}

impl bindings::example::notes::notes::HostWithStore<State> for StateData {
    #[allow(clippy::unused_async_trait_impl)]
    async fn read(store: &Accessor<State, Self>, name: String) -> String {
        store.with(|mut access| {
            access
                .data_mut()
                .notes
                .get(&name)
                .cloned()
                .unwrap_or_else(|| format!("missing note: {name}"))
        })
    }
}

impl bindings::sleeve::platform::audit::Host for StateView<'_> {
    fn log(&mut self, event: String) -> wasmtime::Result<()> {
        self.0.audit.push(event);
        Ok(())
    }
}

impl http_bindings::example::notes::notes::Host for StateView<'_> {}

impl http_bindings::example::notes::notes::HostWithStore<State> for StateData {
    #[allow(clippy::unused_async_trait_impl)]
    async fn read(store: &Accessor<State, Self>, name: String) -> String {
        store.with(|mut access| {
            access
                .data_mut()
                .notes
                .get(&name)
                .cloned()
                .unwrap_or_else(|| format!("missing note: {name}"))
        })
    }
}

impl http_bindings::sleeve::platform::audit::Host for StateView<'_> {
    fn log(&mut self, event: String) -> wasmtime::Result<()> {
        self.0.audit.push(event);
        Ok(())
    }
}

impl http_bindings::sleeve::platform::settings::Host for StateView<'_> {
    fn request_body_limit(&mut self) -> wasmtime::Result<u64> {
        Ok(self.0.request_body_limit)
    }

    fn allowed_origins(&mut self) -> wasmtime::Result<Vec<String>> {
        Ok(self.0.allowed_origins.clone())
    }

    fn preopen_label(&mut self, name: String) -> wasmtime::Result<String> {
        Ok(self
            .0
            .preopen_labels
            .get(&name)
            .cloned()
            .unwrap_or_default())
    }
}

impl file_bindings::sleeve::platform::audit::Host for StateView<'_> {
    fn log(&mut self, event: String) -> wasmtime::Result<()> {
        self.0.audit.push(event);
        Ok(())
    }
}

impl file_bindings::sleeve::platform::settings::Host for StateView<'_> {
    fn request_body_limit(&mut self) -> wasmtime::Result<u64> {
        Ok(self.0.request_body_limit)
    }

    fn allowed_origins(&mut self) -> wasmtime::Result<Vec<String>> {
        Ok(self.0.allowed_origins.clone())
    }

    fn preopen_label(&mut self, name: String) -> wasmtime::Result<String> {
        Ok(self
            .0
            .preopen_labels
            .get(&name)
            .cloned()
            .unwrap_or_default())
    }
}

impl file_bindings::wasi::filesystem::preopens::Host for StateView<'_> {
    fn get_directories(&mut self) -> wasmtime::Result<Vec<(Resource<Descriptor>, String)>> {
        preopens::Host::get_directories(&mut self.0.filesystem())
    }
}

impl file_bindings::wasi::filesystem::types::Host for StateView<'_> {}

impl file_bindings::wasi::filesystem::types::HostDescriptor for StateView<'_> {}

use file_bindings::wasi::clocks::system_clock as file_clock;
use file_bindings::wasi::filesystem::types as file_types;

struct MappedCompletion {
    value: Option<Result<(), file_types::ErrorCode>>,
    closed: bool,
    waker: Option<Waker>,
    hold: bool,
}

struct CompletionConsumer {
    shared: Arc<Mutex<MappedCompletion>>,
}

struct CompletionProducer {
    shared: Arc<Mutex<MappedCompletion>>,
}

fn lock_completion(
    shared: &Mutex<MappedCompletion>,
) -> wasmtime::Result<MutexGuard<'_, MappedCompletion>> {
    shared
        .lock()
        .map_err(|_| wasmtime::Error::msg("filesystem completion relay lock poisoned"))
}

impl FutureConsumer<State> for CompletionConsumer {
    type Item = Result<(), wasi_types::ErrorCode>;

    fn poll_consume(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        mut store: StoreContextMut<State>,
        mut source: Source<'_, Self::Item>,
        finish: bool,
    ) -> Poll<wasmtime::Result<()>> {
        let mut item = None;
        source.read(&mut store, &mut item)?;
        let mut shared = lock_completion(&self.shared)?;
        if let Some(item) = item {
            shared.value = Some(item.map_err(map_error_code));
            shared.closed = true;
            if let Some(waker) = shared.waker.take() {
                waker.wake();
            }
            Poll::Ready(Ok(()))
        } else if finish {
            shared.closed = true;
            if let Some(waker) = shared.waker.take() {
                waker.wake();
            }
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}

impl FutureProducer<State> for CompletionProducer {
    type Item = Result<(), file_types::ErrorCode>;

    fn poll_produce(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        _store: StoreContextMut<State>,
        finish: bool,
    ) -> Poll<wasmtime::Result<Option<Self::Item>>> {
        let mut shared = lock_completion(&self.shared)?;
        if !shared.hold
            && let Some(value) = shared.value.take()
        {
            Poll::Ready(Ok(Some(value)))
        } else if finish || (!shared.hold && shared.closed) {
            Poll::Ready(Ok(None))
        } else {
            shared.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

fn map_completion(
    store: &mut Access<'_, State, StateData>,
    input: FutureReader<Result<(), wasi_types::ErrorCode>>,
) -> wasmtime::Result<FutureReader<Result<(), file_types::ErrorCode>>> {
    let shared = Arc::new(Mutex::new(MappedCompletion {
        value: None,
        closed: false,
        waker: None,
        hold: store.data_mut().hold_file_completions,
    }));
    input.pipe(
        store.as_context_mut(),
        CompletionConsumer {
            shared: Arc::clone(&shared),
        },
    )?;
    FutureReader::new(store.as_context_mut(), CompletionProducer { shared })
}

fn map_error_code(error: wasi_types::ErrorCode) -> file_types::ErrorCode {
    match error {
        wasi_types::ErrorCode::Access => file_types::ErrorCode::Access,
        wasi_types::ErrorCode::Already => file_types::ErrorCode::Already,
        wasi_types::ErrorCode::BadDescriptor => file_types::ErrorCode::BadDescriptor,
        wasi_types::ErrorCode::Busy => file_types::ErrorCode::Busy,
        wasi_types::ErrorCode::Deadlock => file_types::ErrorCode::Deadlock,
        wasi_types::ErrorCode::Quota => file_types::ErrorCode::Quota,
        wasi_types::ErrorCode::Exist => file_types::ErrorCode::Exist,
        wasi_types::ErrorCode::FileTooLarge => file_types::ErrorCode::FileTooLarge,
        wasi_types::ErrorCode::IllegalByteSequence => file_types::ErrorCode::IllegalByteSequence,
        wasi_types::ErrorCode::InProgress => file_types::ErrorCode::InProgress,
        wasi_types::ErrorCode::Interrupted => file_types::ErrorCode::Interrupted,
        wasi_types::ErrorCode::Invalid => file_types::ErrorCode::Invalid,
        wasi_types::ErrorCode::Io => file_types::ErrorCode::Io,
        wasi_types::ErrorCode::IsDirectory => file_types::ErrorCode::IsDirectory,
        wasi_types::ErrorCode::Loop => file_types::ErrorCode::Loop,
        wasi_types::ErrorCode::TooManyLinks => file_types::ErrorCode::TooManyLinks,
        wasi_types::ErrorCode::MessageSize => file_types::ErrorCode::MessageSize,
        wasi_types::ErrorCode::NameTooLong => file_types::ErrorCode::NameTooLong,
        wasi_types::ErrorCode::NoDevice => file_types::ErrorCode::NoDevice,
        wasi_types::ErrorCode::NoEntry => file_types::ErrorCode::NoEntry,
        wasi_types::ErrorCode::NoLock => file_types::ErrorCode::NoLock,
        wasi_types::ErrorCode::InsufficientMemory => file_types::ErrorCode::InsufficientMemory,
        wasi_types::ErrorCode::InsufficientSpace => file_types::ErrorCode::InsufficientSpace,
        wasi_types::ErrorCode::NotDirectory => file_types::ErrorCode::NotDirectory,
        wasi_types::ErrorCode::NotEmpty => file_types::ErrorCode::NotEmpty,
        wasi_types::ErrorCode::NotRecoverable => file_types::ErrorCode::NotRecoverable,
        wasi_types::ErrorCode::Unsupported => file_types::ErrorCode::Unsupported,
        wasi_types::ErrorCode::NoTty => file_types::ErrorCode::NoTty,
        wasi_types::ErrorCode::NoSuchDevice => file_types::ErrorCode::NoSuchDevice,
        wasi_types::ErrorCode::Overflow => file_types::ErrorCode::Overflow,
        wasi_types::ErrorCode::NotPermitted => file_types::ErrorCode::NotPermitted,
        wasi_types::ErrorCode::Pipe => file_types::ErrorCode::Pipe,
        wasi_types::ErrorCode::ReadOnly => file_types::ErrorCode::ReadOnly,
        wasi_types::ErrorCode::InvalidSeek => file_types::ErrorCode::InvalidSeek,
        wasi_types::ErrorCode::TextFileBusy => file_types::ErrorCode::TextFileBusy,
        wasi_types::ErrorCode::CrossDevice => file_types::ErrorCode::CrossDevice,
        wasi_types::ErrorCode::Other(detail) => file_types::ErrorCode::Other(detail),
    }
}

fn map_filesystem_error(error: FilesystemError) -> file_types::ErrorCode {
    match error.downcast() {
        Ok(error) => map_error_code(error),
        Err(_) => file_types::ErrorCode::Io,
    }
}

fn map_descriptor_type(value: wasi_types::DescriptorType) -> file_types::DescriptorType {
    match value {
        wasi_types::DescriptorType::BlockDevice => file_types::DescriptorType::BlockDevice,
        wasi_types::DescriptorType::CharacterDevice => file_types::DescriptorType::CharacterDevice,
        wasi_types::DescriptorType::Directory => file_types::DescriptorType::Directory,
        wasi_types::DescriptorType::Fifo => file_types::DescriptorType::Fifo,
        wasi_types::DescriptorType::SymbolicLink => file_types::DescriptorType::SymbolicLink,
        wasi_types::DescriptorType::RegularFile => file_types::DescriptorType::RegularFile,
        wasi_types::DescriptorType::Socket => file_types::DescriptorType::Socket,
        wasi_types::DescriptorType::Other(detail) => file_types::DescriptorType::Other(detail),
    }
}

fn map_instant(value: wasi_types::Instant) -> file_clock::Instant {
    file_clock::Instant {
        seconds: value.seconds,
        nanoseconds: value.nanoseconds,
    }
}

fn map_descriptor_stat(value: wasi_types::DescriptorStat) -> file_types::DescriptorStat {
    file_types::DescriptorStat {
        type_: map_descriptor_type(value.type_),
        link_count: value.link_count,
        size: value.size,
        data_access_timestamp: value.data_access_timestamp.map(map_instant),
        data_modification_timestamp: value.data_modification_timestamp.map(map_instant),
        status_change_timestamp: value.status_change_timestamp.map(map_instant),
    }
}

fn map_path_flags(value: file_types::PathFlags) -> wasi_types::PathFlags {
    if value.contains(file_types::PathFlags::SYMLINK_FOLLOW) {
        wasi_types::PathFlags::SYMLINK_FOLLOW
    } else {
        wasi_types::PathFlags::empty()
    }
}

fn map_open_flags(value: file_types::OpenFlags) -> wasi_types::OpenFlags {
    let mut mapped = wasi_types::OpenFlags::empty();
    for (source, target) in [
        (file_types::OpenFlags::CREATE, wasi_types::OpenFlags::CREATE),
        (
            file_types::OpenFlags::DIRECTORY,
            wasi_types::OpenFlags::DIRECTORY,
        ),
        (
            file_types::OpenFlags::EXCLUSIVE,
            wasi_types::OpenFlags::EXCLUSIVE,
        ),
        (
            file_types::OpenFlags::TRUNCATE,
            wasi_types::OpenFlags::TRUNCATE,
        ),
    ] {
        if value.contains(source) {
            mapped |= target;
        }
    }
    mapped
}

fn map_descriptor_flags(value: file_types::DescriptorFlags) -> wasi_types::DescriptorFlags {
    let mut mapped = wasi_types::DescriptorFlags::empty();
    for (source, target) in [
        (
            file_types::DescriptorFlags::READ,
            wasi_types::DescriptorFlags::READ,
        ),
        (
            file_types::DescriptorFlags::WRITE,
            wasi_types::DescriptorFlags::WRITE,
        ),
        (
            file_types::DescriptorFlags::FILE_INTEGRITY_SYNC,
            wasi_types::DescriptorFlags::FILE_INTEGRITY_SYNC,
        ),
        (
            file_types::DescriptorFlags::DATA_INTEGRITY_SYNC,
            wasi_types::DescriptorFlags::DATA_INTEGRITY_SYNC,
        ),
        (
            file_types::DescriptorFlags::REQUESTED_WRITE_SYNC,
            wasi_types::DescriptorFlags::REQUESTED_WRITE_SYNC,
        ),
        (
            file_types::DescriptorFlags::MUTATE_DIRECTORY,
            wasi_types::DescriptorFlags::MUTATE_DIRECTORY,
        ),
    ] {
        if value.contains(source) {
            mapped |= target;
        }
    }
    mapped
}

fn filesystem_access<'a>(
    store: &'a mut Access<'_, State, StateData>,
) -> Access<'a, State, WasiFilesystem> {
    Access::new(store.as_context_mut(), |state: &mut State| {
        state.filesystem()
    })
}

impl file_bindings::wasi::filesystem::types::HostDescriptorWithStore<State> for StateData {
    fn read_via_stream(
        mut store: Access<'_, State, Self>,
        descriptor: Resource<Descriptor>,
        offset: u64,
    ) -> wasmtime::Result<(
        StreamReader<u8>,
        FutureReader<Result<(), file_types::ErrorCode>>,
    )> {
        let (stream, completion) = wasi_types::HostDescriptorWithStore::read_via_stream(
            filesystem_access(&mut store),
            descriptor,
            offset,
        )?;
        Ok((stream, map_completion(&mut store, completion)?))
    }

    fn write_via_stream(
        mut store: Access<'_, State, Self>,
        descriptor: Resource<Descriptor>,
        data: StreamReader<u8>,
        offset: u64,
    ) -> wasmtime::Result<FutureReader<Result<(), file_types::ErrorCode>>> {
        let completion = wasi_types::HostDescriptorWithStore::write_via_stream(
            filesystem_access(&mut store),
            descriptor,
            data,
            offset,
        )?;
        map_completion(&mut store, completion)
    }

    fn drop(
        mut store: Access<'_, State, Self>,
        descriptor: Resource<Descriptor>,
    ) -> wasmtime::Result<()> {
        wasi_types::HostDescriptor::drop(&mut store.data_mut().filesystem(), descriptor)
    }

    async fn stat(
        store: &Accessor<State, Self>,
        descriptor: Resource<Descriptor>,
    ) -> wasmtime::Result<Result<file_types::DescriptorStat, file_types::ErrorCode>> {
        let delegate = store.with_getter::<WasiFilesystem>(
            wasmtime_wasi::filesystem::WasiFilesystemView::filesystem,
        );
        Ok(
            wasi_types::HostDescriptorWithStore::stat(&delegate, descriptor)
                .await
                .map(map_descriptor_stat)
                .map_err(map_filesystem_error),
        )
    }

    async fn open_at(
        store: &Accessor<State, Self>,
        descriptor: Resource<Descriptor>,
        path_flags: file_types::PathFlags,
        path: String,
        open_flags: file_types::OpenFlags,
        descriptor_flags: file_types::DescriptorFlags,
    ) -> wasmtime::Result<Result<Resource<Descriptor>, file_types::ErrorCode>> {
        let delegate = store.with_getter::<WasiFilesystem>(
            wasmtime_wasi::filesystem::WasiFilesystemView::filesystem,
        );
        Ok(wasi_types::HostDescriptorWithStore::open_at(
            &delegate,
            descriptor,
            map_path_flags(path_flags),
            path,
            map_open_flags(open_flags),
            map_descriptor_flags(descriptor_flags),
        )
        .await
        .map_err(map_filesystem_error))
    }
}

fn add_filesystem_to_linker(linker: &mut Linker<State>) -> wasmtime::Result<()> {
    file_bindings::wasi::filesystem::preopens::add_to_linker::<_, StateData>(linker, |state| {
        StateView(state)
    })?;
    file_bindings::wasi::filesystem::types::add_to_linker::<_, StateData>(linker, |state| {
        StateView(state)
    })
}

impl WasiHttpView for State {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.http,
            table: &mut self.table,
            hooks: Default::default(),
        }
    }
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl Host {
    /// Creates a host with an in-memory notes service and an approved sleeve pin.
    ///
    /// ```no_run
    /// # fn approved_sleeve() -> Vec<u8> { Vec::new() }
    /// let sleeve = approved_sleeve();
    /// let pin = sleeve_host::sleeve_sha256(&sleeve);
    /// let host = sleeve_host::Host::new([], pin)
    ///     .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    /// # let _ = host;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an engine configuration error when async components are unavailable.
    pub fn new(
        notes: impl IntoIterator<Item = (String, String)>,
        sleeve_sha256: [u8; 32],
    ) -> wasmtime::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model_async(true);
        config.concurrency_support(true);
        Ok(Self {
            engine: Engine::new(&config)?,
            notes: notes.into_iter().collect(),
            sleeve_sha256,
        })
    }

    /// Composes and runs `summarize` in a fresh component instance and store.
    ///
    /// # Errors
    ///
    /// Returns a load refusal, instantiation error, host error, or plugin trap.
    pub async fn summarize(
        &self,
        plugin: &[u8],
        sleeve: &[u8],
        invocation: &str,
        first: &str,
        second: &str,
    ) -> anyhow::Result<InvocationResult> {
        let attempt = self
            .summarize_with_audit(plugin, sleeve, invocation, first, second)
            .await?;
        let value = attempt.value.map_err(anyhow::Error::msg)?;
        Ok(InvocationResult {
            value,
            audit: attempt.audit,
        })
    }

    /// Runs `summarize` while retaining audit records when the plugin traps.
    ///
    /// # Errors
    ///
    /// Returns a load, composition, instantiation, or lifecycle error. A trap
    /// from the plugin export is returned in [`InvocationAttempt::value`].
    pub async fn summarize_with_audit(
        &self,
        plugin: &[u8],
        sleeve: &[u8],
        invocation: &str,
        first: &str,
        second: &str,
    ) -> anyhow::Result<InvocationAttempt> {
        let bytes = compose(plugin, sleeve, self.sleeve_sha256)?;
        let component = Component::from_binary(&self.engine, &bytes).map_err(anyhow_message)?;
        let mut linker = Linker::new(&self.engine);
        bindings::example::notes::notes::add_to_linker::<_, StateData>(&mut linker, |state| {
            StateView(state)
        })
        .map_err(anyhow_message)?;
        bindings::sleeve::platform::audit::add_to_linker::<_, StateData>(&mut linker, |state| {
            StateView(state)
        })
        .map_err(anyhow_message)?;
        let mut store = Store::new(
            &self.engine,
            State {
                notes: self.notes.clone(),
                audit: Vec::new(),
                request_body_limit: 0,
                allowed_origins: Vec::new(),
                preopen_labels: BTreeMap::new(),
                table: ResourceTable::new(),
                http: WasiHttpCtx::new(),
                wasi: WasiCtxBuilder::new().build(),
                hold_file_completions: false,
            },
        );
        let guest = bindings::Composed::instantiate_async(&mut store, &component, &linker)
            .await
            .map_err(anyhow_message)?;
        store
            .run_concurrent(async |accessor| {
                guest
                    .sleeve_platform_lifecycle()
                    .call_start(accessor, invocation.to_owned())
                    .await
            })
            .await
            .map_err(anyhow_message)?
            .map_err(anyhow_message)?;
        let value = run_anchored!(store, guest, |accessor| guest.call_summarize(
            accessor,
            first.to_owned(),
            second.to_owned()
        ));
        let value = match value {
            Ok(Ok(value)) => {
                store
                    .run_concurrent(async |accessor| {
                        guest
                            .sleeve_platform_lifecycle()
                            .call_end(accessor, invocation.to_owned(), false)
                            .await
                    })
                    .await
                    .map_err(anyhow_message)?
                    .map_err(anyhow_message)?;
                Ok(value)
            }
            Ok(Err(error)) | Err(error) => Err(error.to_string()),
        };
        Ok(InvocationAttempt {
            value,
            audit: store.into_data().audit,
        })
    }
}

impl HttpHost {
    /// Creates an HTTP host with notes, an approved sleeve, and policy settings.
    ///
    /// # Errors
    ///
    /// Returns an engine configuration error when async components are unavailable.
    pub fn new(
        notes: impl IntoIterator<Item = (String, String)>,
        sleeve_sha256: [u8; 32],
        request_body_limit: u64,
        allowed_origins: impl IntoIterator<Item = String>,
    ) -> wasmtime::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model_async(true);
        config.concurrency_support(true);
        Ok(Self {
            engine: Engine::new(&config)?,
            notes: notes.into_iter().collect(),
            sleeve_sha256,
            request_body_limit,
            allowed_origins: allowed_origins.into_iter().collect(),
        })
    }

    /// Runs one shared HTTP scenario while retaining audit records after traps.
    ///
    /// # Errors
    ///
    /// Returns a load, composition, instantiation, or lifecycle error. A trap
    /// from the plugin export is returned in [`InvocationAttempt::value`].
    pub async fn run(
        &self,
        plugin: &[u8],
        sleeve: &[u8],
        invocation: &str,
        scenario: u8,
        authority: &str,
        body_size: u32,
    ) -> anyhow::Result<InvocationAttempt> {
        let bytes = compose(plugin, sleeve, self.sleeve_sha256)?;
        let component = Component::from_binary(&self.engine, &bytes).map_err(anyhow_message)?;
        let mut linker = Linker::new(&self.engine);
        http_bindings::example::notes::notes::add_to_linker::<_, StateData>(&mut linker, |state| {
            StateView(state)
        })
        .map_err(anyhow_message)?;
        http_bindings::sleeve::platform::audit::add_to_linker::<_, StateData>(
            &mut linker,
            |state| StateView(state),
        )
        .map_err(anyhow_message)?;
        http_bindings::sleeve::platform::settings::add_to_linker::<_, StateData>(
            &mut linker,
            |state| StateView(state),
        )
        .map_err(anyhow_message)?;
        wasmtime_wasi_http::p3::add_to_linker(&mut linker).map_err(anyhow_message)?;
        let mut store = Store::new(
            &self.engine,
            State {
                notes: self.notes.clone(),
                audit: Vec::new(),
                request_body_limit: self.request_body_limit,
                allowed_origins: self.allowed_origins.clone(),
                preopen_labels: BTreeMap::new(),
                table: ResourceTable::new(),
                http: WasiHttpCtx::new(),
                wasi: WasiCtxBuilder::new().build(),
                hold_file_completions: false,
            },
        );
        let guest = http_bindings::HttpComposed::instantiate_async(&mut store, &component, &linker)
            .await
            .map_err(anyhow_message)?;
        store
            .run_concurrent(async |accessor| {
                guest
                    .sleeve_platform_lifecycle()
                    .call_start(accessor, invocation.to_owned())
                    .await
            })
            .await
            .map_err(anyhow_message)?
            .map_err(anyhow_message)?;
        let value = run_anchored!(store, guest, |accessor| guest.call_run(
            accessor,
            scenario,
            authority.to_owned(),
            body_size
        ));
        let value = match value {
            Ok(Ok(value)) => {
                store
                    .run_concurrent(async |accessor| {
                        guest
                            .sleeve_platform_lifecycle()
                            .call_end(accessor, invocation.to_owned(), false)
                            .await
                    })
                    .await
                    .map_err(anyhow_message)?
                    .map_err(anyhow_message)?;
                Ok(value)
            }
            Ok(Err(error)) | Err(error) => Err(error.to_string()),
        };
        Ok(InvocationAttempt {
            value,
            audit: store.into_data().audit,
        })
    }
}

impl FileHost {
    /// Creates a filesystem host with an approved sleeve and labeled preopens.
    ///
    /// # Errors
    ///
    /// Returns an engine configuration error when async components are unavailable.
    pub fn new(
        sleeve_sha256: [u8; 32],
        preopens: impl IntoIterator<Item = FilePreopen>,
    ) -> wasmtime::Result<Self> {
        let mut preopens = preopens.into_iter().collect::<Vec<_>>();
        for preopen in &mut preopens {
            preopen.path = std::fs::canonicalize(&preopen.path)
                .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
        }
        for (index, preopen) in preopens.iter().enumerate() {
            for other in &preopens[index + 1..] {
                if preopen.path.starts_with(&other.path) || other.path.starts_with(&preopen.path) {
                    return Err(wasmtime::Error::msg(format!(
                        "filesystem preopens overlap: `{}` and `{}`",
                        preopen.path.display(),
                        other.path.display()
                    )));
                }
            }
        }

        let mut config = Config::new();
        config.wasm_component_model_async(true);
        config.concurrency_support(true);
        Ok(Self {
            engine: Engine::new(&config)?,
            sleeve_sha256,
            preopens,
            hold_file_completions: false,
        })
    }

    /// Runs one filesystem scenario while retaining audit records after traps.
    ///
    /// # Errors
    ///
    /// Returns a preopen, load, composition, instantiation, or lifecycle error.
    /// A trap from the plugin export is returned in [`InvocationAttempt::value`].
    pub async fn run(
        &self,
        plugin: &[u8],
        sleeve: &[u8],
        invocation: &str,
        scenario: u8,
    ) -> anyhow::Result<InvocationAttempt> {
        let bytes = compose(plugin, sleeve, self.sleeve_sha256)?;
        let component = Component::from_binary(&self.engine, &bytes).map_err(anyhow_message)?;
        let mut linker = Linker::new(&self.engine);
        file_bindings::sleeve::platform::audit::add_to_linker::<_, StateData>(
            &mut linker,
            |state| StateView(state),
        )
        .map_err(anyhow_message)?;
        file_bindings::sleeve::platform::settings::add_to_linker::<_, StateData>(
            &mut linker,
            |state| StateView(state),
        )
        .map_err(anyhow_message)?;
        add_filesystem_to_linker(&mut linker).map_err(anyhow_message)?;

        let mut wasi = WasiCtxBuilder::new();
        let mut labels = BTreeMap::new();
        for preopen in &self.preopens {
            wasi.preopened_dir(&preopen.path, &preopen.name, FsPerms::ReadWrite)
                .map_err(anyhow_message)?;
            labels.insert(preopen.name.clone(), preopen.label.clone());
        }
        let mut store = Store::new(
            &self.engine,
            State {
                notes: BTreeMap::new(),
                audit: Vec::new(),
                request_body_limit: 0,
                allowed_origins: Vec::new(),
                preopen_labels: labels,
                table: ResourceTable::new(),
                http: WasiHttpCtx::new(),
                wasi: wasi.build(),
                hold_file_completions: self.hold_file_completions,
            },
        );
        let guest = file_bindings::FileComposed::instantiate_async(&mut store, &component, &linker)
            .await
            .map_err(anyhow_message)?;
        store
            .run_concurrent(async |accessor| {
                guest
                    .sleeve_platform_lifecycle()
                    .call_start(accessor, invocation.to_owned())
                    .await
            })
            .await
            .map_err(anyhow_message)?
            .map_err(anyhow_message)?;
        let value = run_anchored!(store, guest, |accessor| guest.call_run(accessor, scenario));
        let value = match value {
            Ok(Ok(value)) => {
                store
                    .run_concurrent(async |accessor| {
                        guest
                            .sleeve_platform_lifecycle()
                            .call_end(accessor, invocation.to_owned(), false)
                            .await
                    })
                    .await
                    .map_err(anyhow_message)?
                    .map_err(anyhow_message)?;
                Ok(value)
            }
            Ok(Err(error)) | Err(error) => Err(error.to_string()),
        };
        Ok(InvocationAttempt {
            value,
            audit: store.into_data().audit,
        })
    }
}

fn anyhow_message(error: impl std::fmt::Display) -> anyhow::Error {
    anyhow::anyhow!(error.to_string())
}
