#![allow(unused)]
pub mod bindings {
    use wit_bindgen::generate;

    generate!({
        world: "guest",
        path: "interface.wit",
        async: {
            exports: [
                "pkg:component/intf#test",
                "pkg:component/intf#test2",
                "pkg:component/intf#test3",
                "pkg:component/intf#[method]session.infer",
            ]
        }
    });

    pub struct Component;

    export!(Component);
}

use bindings::exports::pkg::component::intf::Guest;
use bindings::exports::pkg::component::intf::GuestSession;
use bindings::exports::pkg::component::intf::Request;
use bindings::exports::pkg::component::intf::Response;
use bindings::exports::pkg::component::intf::SessionBorrow;
use wit_bindgen::rt::async_support;
use wit_bindgen::rt::async_support::futures::SinkExt;
use wit_bindgen::rt::async_support::FutureReader;

pub struct Session {
    last_response: String,
}

impl GuestSession for Session {
    fn new() -> Self {
        Self {
            last_response: String::new(),
        }
    }

    async fn infer(&self, prompt: Request) -> Response {
        todo!()
    }
}

impl Guest for bindings::Component {
    type Session = Session;

    async fn test(test: String) -> String {
        format!("Hello World! (test1)")
    }

    async fn test2(test: String) -> FutureReader<String> {
        let (tx, rx) = bindings::wit_future::new::<String>();
        async_support::spawn(async move {
            let response: String = "Hello World! (test2)".to_owned();
            tx.write(response).await;
        });
        rx
    }

    async fn test3(test: FutureReader<String>) -> String {
        test.await.unwrap().unwrap()
    }
}
