#![allow(unused_imports)]
use anyhow::Result;
use std::borrow::Cow;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use wasmtime::component::Accessor;
use wasmtime::component::AccessorTask;
use wasmtime::component::Component;
use wasmtime::component::ComponentExportIndex;
use wasmtime::component::ErrorContext;
use wasmtime::component::HostFuture;
use wasmtime::component::HostStream;
use wasmtime::component::Instance;
use wasmtime::component::Linker;
use wasmtime::component::PromisesUnordered;
use wasmtime::component::ResourceTable;
use wasmtime::component::StreamReader;
use wasmtime::component::StreamWriter;
use wasmtime::component::TypedFunc;
use wasmtime::component::VecBuffer;
use wasmtime::CacheStore;
use wasmtime::Config;
use wasmtime::Engine;
use wasmtime::Store;
use wasmtime_wasi::p3::cli::WasiCliCtx;
use wasmtime_wasi::p3::cli::WasiCliView;
use wasmtime_wasi::p3::clocks::WasiClocksCtx;
use wasmtime_wasi::p3::clocks::WasiClocksView;
use wasmtime_wasi::p3::filesystem::DirPerms;
use wasmtime_wasi::p3::filesystem::FilePerms;
use wasmtime_wasi::p3::filesystem::WasiFilesystemCtx;
use wasmtime_wasi::p3::filesystem::WasiFilesystemView;
use wasmtime_wasi::p3::random::WasiRandomCtx;
use wasmtime_wasi::p3::random::WasiRandomView;
use wasmtime_wasi::p3::sockets::AllowedNetworkUses;
use wasmtime_wasi::p3::sockets::SocketAddrCheck;
use wasmtime_wasi::p3::sockets::WasiSocketsCtx;
use wasmtime_wasi::p3::sockets::WasiSocketsView;
use wasmtime_wasi::p3::AccessorTaskFn;
use wasmtime_wasi::p3::ResourceView;
use wasmtime_wasi::IoView;
use wasmtime_wasi::WasiCtx;
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::WasiView;

pub const GUEST: &str = concat!(env!("OUT_DIR"), "/wasm32-wasip1/debug/guest.component.wasm");

pub struct Host {
    ctx: WasiCtx,
    table: ResourceTable,
    sockets: WasiSocketsCtx,
    random: WasiRandomCtx,
    clocks: WasiClocksCtx,
    cli: WasiCliCtx,
    filesystem: WasiFilesystemCtx,
}

impl WasiView for Host {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

impl IoView for Host {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

impl ResourceView for Host {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

impl WasiClocksView for Host {
    fn clocks(&mut self) -> &WasiClocksCtx {
        &self.clocks
    }
}

impl WasiCliView for Host {
    fn cli(&mut self) -> &WasiCliCtx {
        &self.cli
    }
}

impl WasiRandomView for Host {
    fn random(&mut self) -> &mut WasiRandomCtx {
        &mut self.random
    }
}

impl WasiSocketsView for Host {
    fn sockets(&self) -> &WasiSocketsCtx {
        &self.sockets
    }
}

impl WasiFilesystemView for Host {
    fn filesystem(&self) -> &WasiFilesystemCtx {
        &self.filesystem
    }
}

#[derive(Debug)]
struct Cache;

static CACHE: Mutex<Option<HashMap<Vec<u8>, Vec<u8>>>> = Mutex::new(None);

impl CacheStore for Cache {
    fn get(&self, key: &[u8]) -> Option<Cow<[u8]>> {
        let mut cache = CACHE.lock().unwrap();
        let cache = cache.get_or_insert_with(HashMap::new);
        cache.get(key).map(|s| s.to_vec().into())
    }

    fn insert(&self, key: &[u8], value: Vec<u8>) -> bool {
        let mut cache = CACHE.lock().unwrap();
        let cache = cache.get_or_insert_with(HashMap::new);
        cache.insert(key.to_vec(), value);
        true
    }
}

pub async fn init() -> (Instance, Store<Host>, ComponentExportIndex) {
    let mut config = Config::new();
    config.async_support(true);
    config.wasm_component_model_async(true);
    config.enable_incremental_compilation(Arc::new(Cache)).unwrap();
    config.cache_config_load("config.toml").unwrap();

    let mut host = Host {
        table: ResourceTable::new(),
        sockets: WasiSocketsCtx::default(),
        random: WasiRandomCtx::default(),
        clocks: WasiClocksCtx::default(),
        cli: WasiCliCtx::default(),
        filesystem: WasiFilesystemCtx::default(),
        ctx: WasiCtxBuilder::new().inherit_stdio().build(),
    };

    host.filesystem
        .preopened_dir("data", "data", DirPerms::READ, FilePerms::READ)
        .unwrap();
    host.sockets.socket_addr_check = SocketAddrCheck::new(|_, _| Box::pin(async { true }));
    host.sockets.allowed_network_uses = AllowedNetworkUses {
        ip_name_lookup: true,
        udp: true,
        tcp: true,
    };

    let engine = Engine::new(&config).unwrap();
    let component = Component::from_file(&engine, &GUEST).unwrap();
    let mut store = Store::new(&engine, host);
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker).unwrap();
    wasmtime_wasi::p3::sockets::add_to_linker(&mut linker).unwrap();
    wasmtime_wasi::p3::random::add_to_linker(&mut linker).unwrap();
    wasmtime_wasi::p3::clocks::add_to_linker(&mut linker).unwrap();
    wasmtime_wasi::p3::cli::add_to_linker(&mut linker).unwrap();
    wasmtime_wasi::p3::filesystem::add_to_linker::<Host>(&mut linker).unwrap();

    let instance = linker
        .instantiate_async(&mut store, &component)
        .await
        .unwrap();

    let intf_export = instance
        .get_export(&mut store, None, "pkg:component/intf")
        .unwrap();

    (instance, store, intf_export)
}

// test1: async fn(String) -> String
#[tokio::test]
async fn test1() {
    let (instance, mut store, intf_export) = init().await;
    let export = instance
        .get_export(&mut store, Some(&intf_export), "test")
        .unwrap();
    let func: TypedFunc<(String,), (String,)> =
        instance.get_typed_func(&mut store, export).unwrap();

    let (result,) = func
        .call_async(&mut store, ("Hello".to_owned(),))
        .await
        .unwrap();

    func.post_return_async(&mut store).await.unwrap();

    println!("Result: {:?}", result);
}

// test2: async fn<String> -> Future<String>
#[tokio::test]
async fn test2() {
    let (instance, mut store, intf_export) = init().await;
    let export = instance
        .get_export(&mut store, Some(&intf_export), "test2")
        .unwrap();

    let func2: TypedFunc<(String,), (HostFuture<String>,)> =
        instance.get_typed_func(&mut store, export).unwrap();

    let (result,) = func2
        .call_async(&mut store, ("Hello".to_owned(),))
        .await
        .unwrap();

    func2.post_return_async(&mut store).await.unwrap();

    if let Ok(Some(result)) = result.into_reader(&mut store).read().get(&mut store).await {
        println!("Result: {:?}", result);
    }
}

// test3: async fn(Future<String>) -> String
#[tokio::test]
async fn test3() {
    let (instance, mut store, intf_export) = init().await;
    let export = instance
        .get_export(&mut store, Some(&intf_export), "test3")
        .unwrap();

    let func3: TypedFunc<(HostFuture<String>,), (String,)> =
        instance.get_typed_func(&mut store, export).unwrap();

    let (tx, rx) = instance.future(&mut store).unwrap();

    let handle = tokio::task::spawn(async move {
        tx.write("Hello World! (test3)".to_owned())
            .into_future()
            .await;
    });

    let (result,) = func3.call_async(&mut store, (rx.into(),)).await.unwrap();

    func3.post_return_async(&mut store).await.unwrap();

    println!("Result: {:?}", result);

    handle.await.unwrap();
}

// test4: async fn(Stream<String>) -> Stream<String>
#[tokio::test]
async fn test4() {
    let (instance, mut store, intf_export) = init().await;
    let export = instance
        .get_export(&mut store, Some(&intf_export), "test4")
        .unwrap();

    enum Event {
        Write((Option<StreamWriter<VecBuffer<String>>>, VecBuffer<String>)),
        Read((Option<StreamReader<Vec<String>>>, Vec<String>)),
    }

    let mut set = PromisesUnordered::<Event>::new();

    let func3: TypedFunc<(HostStream<String>,), (HostStream<String>,)> =
        instance.get_typed_func(&mut store, export).unwrap();

    let buf = Vec::with_capacity(1024);
    let (tx, rx) = instance
        .stream::<String, VecBuffer<String>, Vec<String>, _, _>(&mut store)
        .unwrap();

    let (result,) = func3.call_async(&mut store, (rx.into(),)).await.unwrap();

    set.push(
        tx.write(VecBuffer::from(vec!["Hello World! (test4)".to_owned()]))
            .map(Event::Write),
    );
    set.push(result.into_reader(&mut store).read(buf).map(Event::Read));

    func3.post_return_async(&mut store).await.unwrap();

    while let Ok(Some(event)) = set.next(&mut store).await {
        match event {
            Event::Write((Some(tx), _)) => {
                println!("Writing");
                set.push(
                    tx.write(VecBuffer::from(vec!["Hello World! (test4)".to_owned()]))
                        .map(Event::Write),
                );
            }
            Event::Write(_) => {
                println!("Write finished");
            }
            Event::Read((Some(reader), buf)) => {
                println!("Reading: {:?}", buf);
                set.push(reader.read(buf).map(Event::Read));
            }
            Event::Read(_) => {
                println!("Read error");
            }
        }
    }
    println!("All done");
}

// get-files: async fn() -> String
#[tokio::test]
async fn test_get_files_p3() {
    let (instance, mut store, intf_export) = init().await;
    let export = instance
        .get_export(&mut store, Some(&intf_export), "get-files-p3")
        .unwrap();
    let func: TypedFunc<(), (String,)> = instance.get_typed_func(&mut store, export).unwrap();
    let (result,) = func.call_async(&mut store, ()).await.unwrap();
    func.post_return_async(&mut store).await.unwrap();
    println!("Result: {}", result);
}
