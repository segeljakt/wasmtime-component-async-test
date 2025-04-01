#![allow(unused_imports)]
use anyhow::Result;
use std::error::Error;
use wasmtime::component::ErrorContext;

use wasmtime::component::Accessor;
use wasmtime::component::AccessorTask;
use wasmtime::component::Component;
use wasmtime::component::ComponentExportIndex;
use wasmtime::component::HostFuture;
use wasmtime::component::HostStream;
use wasmtime::component::Instance;
use wasmtime::component::Linker;
use wasmtime::component::PromisesUnordered;
use wasmtime::component::ResourceTable;
use wasmtime::component::Single;
use wasmtime::component::StreamReader;
use wasmtime::component::StreamWriter;
use wasmtime::component::TypedFunc;
use wasmtime::Config;
use wasmtime::Engine;
use wasmtime::Store;
use wasmtime_wasi::p3::AccessorTaskFn;
use wasmtime_wasi::DirPerms;
use wasmtime_wasi::FilePerms;
use wasmtime_wasi::IoImpl;
use wasmtime_wasi::IoView;
use wasmtime_wasi::WasiCtx;
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::WasiImpl;
use wasmtime_wasi::WasiView;

pub const GUEST: &str = concat!(env!("OUT_DIR"), "/wasm32-wasip1/debug/guest.component.wasm");

pub struct Host {
    ctx: WasiCtx,
    table: ResourceTable,
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

pub async fn init() -> (Instance, Store<WasiImpl<Host>>, ComponentExportIndex) {
    let mut config = Config::new();
    config.debug_info(true);
    config.cranelift_debug_verifier(true);
    config.async_support(true);
    config.wasm_component_model(true);
    config.wasm_component_model_async(true);

    let ctx = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_network()
        .preopened_dir("data", "data", DirPerms::READ, FilePerms::READ)
        .unwrap()
        .build();
    let table = ResourceTable::new();
    let host = Host { ctx, table };

    let engine = Engine::new(&config).unwrap();
    let component = Component::from_file(&engine, &GUEST).unwrap();
    let host = WasiImpl(IoImpl(host));
    let mut store = Store::new(&engine, host);
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async::<WasiImpl<Host>>(&mut linker).unwrap();

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

    if let Ok(Ok(result)) = result.into_reader(&mut store).read().get(&mut store).await {
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

// struct Task {
//     tx: StreamWriter<Single<String>>,
// }
//
// impl<T, U: IoView> AccessorTask<T, U, Result<()>> for Task {
//     async fn run(self, accessor: &mut Accessor<T, U>) -> Result<()> {
//         let mut tx = Some(self.tx);
//         for _ in 0..10 {
//             tx = accessor
//                 .with(|_view| {
//                     Ok::<_, anyhow::Error>(
//                         tx.take()
//                             .unwrap()
//                             .write(Single("Hello".to_string()))
//                             .into_future(),
//                     )
//                 })?
//                 .await;
//         }
//         Ok(())
//     }
// }

// test4: async fn(Stream<String>) -> Stream<String>
#[tokio::test]
async fn test4() {
    let (instance, mut store, intf_export) = init().await;
    let export = instance
        .get_export(&mut store, Some(&intf_export), "test4")
        .unwrap();

    enum Event {
        Write(Option<StreamWriter<Single<String>>>),
        Read(Result<(StreamReader<Single<String>>, Single<String>), Option<ErrorContext>>),
    }

    let mut set = PromisesUnordered::<Event>::new();

    let func3: TypedFunc<(HostStream<String>,), (HostStream<String>,)> =
        instance.get_typed_func(&mut store, export).unwrap();

    let (tx, rx) = instance.stream(&mut store).unwrap();

    let (result,) = func3.call_async(&mut store, (rx.into(),)).await.unwrap();

    set.push(
        tx.write(Single("Hello World! (test4)".to_owned()))
            .map(Event::Write),
    );
    set.push(result.into_reader(&mut store).read().map(Event::Read));

    func3.post_return_async(&mut store).await.unwrap();

    while let Ok(Some(event)) = set.next(&mut store).await {
        match event {
            Event::Write(Some(tx)) => {
                println!("Writing");
                set.push(
                    tx.write(Single("Hello World! (test4)".to_owned()))
                        .map(Event::Write),
                );
            }
            Event::Write(None) => {
                println!("Write finished");
            }
            Event::Read(Ok((reader, v))) => {
                println!("Reading: {:?}", v.0);
                set.push(reader.read().map(Event::Read));
            }
            Event::Read(Err(_)) => {
                println!("Read error");
            }
        }
    }
    println!("All done");
}

// get-files: async fn() -> String
#[tokio::test]
async fn test_get_files() {
    let (instance, mut store, intf_export) = init().await;
    let export = instance
        .get_export(&mut store, Some(&intf_export), "get-files")
        .unwrap();
    let func: TypedFunc<(), (Vec<String>,)> = instance.get_typed_func(&mut store, export).unwrap();
    let (result,) = func.call_async(&mut store, ()).await.unwrap();
    func.post_return_async(&mut store).await.unwrap();
    println!("Result: {:?}", result);
}

// read-file: async fn() -> String
#[tokio::test]
async fn test_read_file() {
    let (instance, mut store, intf_export) = init().await;
    let export = instance
        .get_export(&mut store, Some(&intf_export), "read-file")
        .unwrap();
    let func: TypedFunc<(), (String,)> = instance.get_typed_func(&mut store, export).unwrap();
    let (result,) = func.call_async(&mut store, ()).await.unwrap();
    func.post_return_async(&mut store).await.unwrap();
    println!("Result: {}", result);
}
