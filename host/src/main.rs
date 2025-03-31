use wasmtime::component::Component;
use wasmtime::component::HostFuture;
use wasmtime::component::Linker;
use wasmtime::component::ResourceTable;
use wasmtime::component::TypedFunc;
use wasmtime::Config;
use wasmtime::Engine;
use wasmtime::Store;
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
        let ctx = WasiCtxBuilder::new().inherit_stdio().build();
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

    {
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

    {
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

    {
        let export = instance
            .get_export(&mut store, Some(&intf_export), "test3")
            .unwrap();

        let func3: TypedFunc<(HostFuture<String>,), (String,)> =
            instance.get_typed_func(&mut store, export).unwrap();

        let (tx, rx) = instance.future(&mut store).unwrap();

        tokio::task::spawn(async move {
            tx.write("Hello World! (test3)".to_owned()).into_future().await;
        });

        let (result,) = func3.call_async(&mut store, (rx.into(),)).await.unwrap();

        func3.post_return_async(&mut store).await.unwrap();

        println!("Result: {:?}", result);
    }
    Ok(())
}
