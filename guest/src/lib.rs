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
                "pkg:component/intf#test4",
                "pkg:component/intf#get-files",
                "pkg:component/intf#read-file",
                "pkg:component/intf#[method]session.infer",
            ]
        }
    });

    pub struct Component;

    export!(Component);
}

use std::io::Read;

use bindings::exports::pkg::component::intf::Guest;
use bindings::exports::pkg::component::intf::GuestSession;
use bindings::exports::pkg::component::intf::Request;
use bindings::exports::pkg::component::intf::Response;
use bindings::exports::pkg::component::intf::SessionBorrow;
use wasi::cli::stdin::InputStream;
use wasi::filesystem::preopens::get_directories;
use wasi::filesystem::types::Descriptor;
use wasi::filesystem::types::DescriptorFlags;
use wasi::filesystem::types::Filesize;
use wasi::filesystem::types::OpenFlags;
use wasi::filesystem::types::PathFlags;
use wit_bindgen::rt::async_support;
use wit_bindgen::rt::async_support::futures::SinkExt;
use wit_bindgen::rt::async_support::futures::StreamExt;
use wit_bindgen::rt::async_support::FutureReader;
use wit_bindgen::rt::async_support::StreamReader;

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

    async fn test4(mut stream: StreamReader<String>) -> StreamReader<String> {
        let (mut tx, rx) = bindings::wit_stream::new::<String>();
        async_support::spawn(async move {
            for i in 0..10 {
                match stream.next().await {
                    Some(Ok(_items)) => {
                        tx.send(vec!["Response".to_string()]).await;
                    }
                    _ => {
                        tx.close().await.unwrap();
                        break;
                    }
                }
            }
        });
        rx
    }

    async fn get_files() -> Vec<String> {
        get_directories()
            .into_iter()
            .flat_map(|(desc, name)| {
                let stream = desc.read_directory().unwrap();
                std::iter::from_fn(move || stream.read_directory_entry().unwrap())
                    .map(|entry| entry.name)
            })
            .collect::<Vec<_>>()
    }

    async fn read_file() -> String {
        let dirs = get_directories();
        let (dir, _) = dirs.first().unwrap();
        let mut string = String::new();
        dir.open_at(
            PathFlags::empty(),
            "bids.csv",
            OpenFlags::empty(),
            DescriptorFlags::READ,
        )
        .unwrap()
        .read_via_stream(0)
        .unwrap()
        .read_to_string(&mut string)
        .unwrap();
        string
    }
}
