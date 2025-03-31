#![allow(unused)]
pub mod bindings {
    use wit_bindgen_rust_macro::generate;

    generate!({
        world: "guest",
        path: "interface.wit",
        async: {
            exports: [
                "pkg:component/intf#test",
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
use wit_bindgen::rt::async_support::FutureReader;
use wit_bindgen::rt::async_support;
use wit_bindgen::rt::async_support::futures::SinkExt;

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
        // let (tx, rx) = bindings::wit_future::new();
        // async_support::spawn(async move {
        //     let response = Response {
        //         message: format!("Response to: {}", prompt.message)
        //     };
        //     tx.send(response).await;
        // });

        // tx.write("".into());

        // rx
        Response {
            message: format!("Response to: {}", prompt.message),
        }
    }
}

impl Guest for bindings::Component {
    type Session = Session;

    async fn test(test: String) -> String {
        format!("Hello world!")
    }
}
