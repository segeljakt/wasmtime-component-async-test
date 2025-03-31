use wasmtime::component::Component;
use wasmtime::component::Linker;
use wasmtime::component::ResourceTable;
use wasmtime::component::TypedFunc;
use wasmtime::Config;
use wasmtime::Engine;
use wasmtime::Store;
use wasmtime_wasi::IoView;
use wasmtime_wasi::WasiCtx;
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::WasiImpl;
use wasmtime_wasi::WasiView;

const GUEST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../guest/target/wasm32-wasip1/release/guest.wasm"
);

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
    let host = WasiImpl(wasmtime_wasi::IoImpl(Host::new()));
    let mut store = Store::new(&engine, host);
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async::<WasiImpl<Host>>(&mut linker).unwrap();

    let instance = linker.instantiate(&mut store, &component).unwrap();
    let intf_export = instance.get_export(&mut store, None, "intf").unwrap();

    let _prompt = std::env::args()
        .nth(1)
        .unwrap_or("Hello, my name is".to_string());

    let export = instance
        .get_export(&mut store, Some(&intf_export), "test")
        .unwrap();
    let func: TypedFunc<(), (String,)> = instance.get_typed_func(&mut store, export).unwrap();
    let result = func.call_async(&mut store, ()).await.unwrap();

    func.post_return(&mut store).unwrap();

    println!("Result: {:?}", result);

    Ok(())
}
