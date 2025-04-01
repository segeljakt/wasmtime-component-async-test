#![allow(unused)]
use wasmtime::component::Component;
use wasmtime::component::ComponentExportIndex;
use wasmtime::component::HostFuture;
use wasmtime::component::HostStream;
use wasmtime::component::Instance;
use wasmtime::component::Linker;
use wasmtime::component::ResourceTable;
use wasmtime::component::Single;
use wasmtime::component::StreamReader;
use wasmtime::component::TypedFunc;
use wasmtime::Config;
use wasmtime::Engine;
use wasmtime::Store;
use wasmtime_wasi::DirPerms;
use wasmtime_wasi::FilePerms;
use wasmtime_wasi::IoImpl;
use wasmtime_wasi::IoView;
use wasmtime_wasi::WasiCtx;
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::WasiImpl;
use wasmtime_wasi::WasiView;

const GUEST: &str = concat!(env!("OUT_DIR"), "/wasm32-wasip1/debug/guest.component.wasm");

struct Host {
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

impl Host {
    fn new() -> Self {
        let ctx = WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_network()
            .preopened_dir("data", "data", DirPerms::READ, FilePerms::READ)
            .unwrap()
            .build();
        let table = ResourceTable::new();
        Self { ctx, table }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut config = Config::new();
    config.debug_info(true);
    config.cranelift_debug_verifier(true);
    config.async_support(true);
    config.wasm_component_model(true);
    config.wasm_component_model_async(true);
    config.wasm_component_model_async_builtins(true);
    config.wasm_component_model_async_stackful(true);

    let engine = Engine::new(&config).unwrap();
    let component = Component::from_file(&engine, &GUEST).unwrap();
    let host = WasiImpl(IoImpl(Host::new()));
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

    test1(&instance, &mut store, &intf_export).await;
    test2(&instance, &mut store, &intf_export).await;
    test3(&instance, &mut store, &intf_export).await;
    // test4(&instance, &mut store, &intf_export).await;
    test_get_files(&instance, &mut store, &intf_export).await;
    test_read_file(&instance, &mut store, &intf_export).await;
    Ok(())
}

// test1: async fn(String) -> String
async fn test1(
    instance: &Instance,
    mut store: &mut Store<WasiImpl<Host>>,
    intf_export: &ComponentExportIndex,
) {
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
async fn test2(
    instance: &Instance,
    mut store: &mut Store<WasiImpl<Host>>,
    intf_export: &ComponentExportIndex,
) {
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
async fn test3(
    instance: &Instance,
    mut store: &mut Store<WasiImpl<Host>>,
    intf_export: &ComponentExportIndex,
) {
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
async fn test4(
    instance: &Instance,
    mut store: &mut Store<WasiImpl<Host>>,
    intf_export: &ComponentExportIndex,
) {
    let export = instance
        .get_export(&mut store, Some(&intf_export), "test4")
        .unwrap();

    let func3: TypedFunc<(HostStream<String>,), (HostStream<String>,)> =
        instance.get_typed_func(&mut store, export).unwrap();

    let (mut tx, rx) = instance.stream(&mut store).unwrap();

    let (result,) = func3.call_async(&mut store, (rx.into(),)).await.unwrap();

    let handle1 = tokio::task::spawn(async move {
        for i in 0..10 {
            println!("{i} Writing: Hello World! (test4)");
            let Some(new) = tx
                .write(Single(format!("Hello World! {i} (test4)")))
                .into_future()
                .await
            else {
                panic!("Error writing stream");
            };
            tx = new;
        }
    });

    func3.post_return_async(&mut store).await.unwrap();
    let mut result: StreamReader<Vec<String>> = result.into_reader(&mut store);

    let handle2 = tokio::task::spawn(async move {
        for i in 0..10 {
            println!("{i} Reading...");
            let Ok((new, item)) = result.read().into_future().await else {
                panic!("Error reading stream");
            };

            result = new;

            println!("Result: {:?}", item);
        }
    });
    tokio::try_join!(handle1, handle2).unwrap();
}

// get-files: async fn() -> String
async fn test_get_files(
    instance: &Instance,
    mut store: &mut Store<WasiImpl<Host>>,
    intf_export: &ComponentExportIndex,
) {
    let export = instance
        .get_export(&mut store, Some(&intf_export), "get-files")
        .unwrap();
    let func: TypedFunc<(), (Vec<String>,)> = instance.get_typed_func(&mut store, export).unwrap();
    let (result,) = func.call_async(&mut store, ()).await.unwrap();
    func.post_return_async(&mut store).await.unwrap();
    println!("Result: {:?}", result);
}

// read-file: async fn() -> String
async fn test_read_file(
    instance: &Instance,
    mut store: &mut Store<WasiImpl<Host>>,
    intf_export: &ComponentExportIndex,
) {
    let export = instance
        .get_export(&mut store, Some(&intf_export), "read-file")
        .unwrap();
    let func: TypedFunc<(), (String,)> = instance.get_typed_func(&mut store, export).unwrap();
    let (result,) = func.call_async(&mut store, ()).await.unwrap();
    func.post_return_async(&mut store).await.unwrap();
    println!("Result: {}", result);
}
