#![allow(unused)]
pub mod bindings {
    wit_bindgen::generate!({
        world: "pkg:component/guest",
        path: [
            // Note: These imports are order-sensitive.
            "../wasip3-prototyping/crates/wasi/src/p3/wit/",
            "interface.wit",
        ],
        async: {
            exports: [
                "pkg:component/intf#test",
                "pkg:component/intf#test2",
                "pkg:component/intf#test3",
                "pkg:component/intf#test4",
                "pkg:component/intf#[method]session.infer",
                "pkg:component/intf#get-files-p3",
            ],
            imports: [
                "wasi:cli/stdin@0.3.0#get-stdin",
                "wasi:cli/stdout@0.3.0#set-stdout",
                "wasi:cli/stderr@0.3.0#set-stderr",
                "wasi:clocks/monotonic-clock@0.3.0#wait-for",
                "wasi:clocks/monotonic-clock@0.3.0#wait-until",
                "wasi:filesystem/types@0.3.0#[method]descriptor.read-via-stream",
                "wasi:filesystem/types@0.3.0#[method]descriptor.write-via-stream",
                "wasi:filesystem/types@0.3.0#[method]descriptor.append-via-stream",
                "wasi:filesystem/types@0.3.0#[method]descriptor.advise",
                "wasi:filesystem/types@0.3.0#[method]descriptor.sync-data",
                "wasi:filesystem/types@0.3.0#[method]descriptor.get-flags",
                "wasi:filesystem/types@0.3.0#[method]descriptor.get-type",
                "wasi:filesystem/types@0.3.0#[method]descriptor.set-size",
                "wasi:filesystem/types@0.3.0#[method]descriptor.set-times",
                "wasi:filesystem/types@0.3.0#[method]descriptor.read-directory",
                "wasi:filesystem/types@0.3.0#[method]descriptor.sync",
                "wasi:filesystem/types@0.3.0#[method]descriptor.create-directory-at",
                "wasi:filesystem/types@0.3.0#[method]descriptor.stat",
                "wasi:filesystem/types@0.3.0#[method]descriptor.stat-at",
                "wasi:filesystem/types@0.3.0#[method]descriptor.set-times-at",
                "wasi:filesystem/types@0.3.0#[method]descriptor.link-at",
                "wasi:filesystem/types@0.3.0#[method]descriptor.open-at",
                "wasi:filesystem/types@0.3.0#[method]descriptor.readlink-at",
                "wasi:filesystem/types@0.3.0#[method]descriptor.remove-directory-at",
                "wasi:filesystem/types@0.3.0#[method]descriptor.rename-at",
                "wasi:filesystem/types@0.3.0#[method]descriptor.symlink-at",
                "wasi:filesystem/types@0.3.0#[method]descriptor.unlink-file-at",
                "wasi:filesystem/types@0.3.0#[method]descriptor.is-same-object",
                "wasi:filesystem/types@0.3.0#[method]descriptor.metadata-hash",
                "wasi:filesystem/types@0.3.0#[method]descriptor.metadata-hash-at",
                "wasi:sockets/ip-name-lookup@0.3.0#resolve-addresses",
                "wasi:sockets/types@0.3.0#[method]tcp-socket.bind",
                "wasi:sockets/types@0.3.0#[method]tcp-socket.connect",
                "wasi:sockets/types@0.3.0#[method]tcp-socket.listen",
                "wasi:sockets/types@0.3.0#[method]tcp-socket.receive",
                "wasi:sockets/types@0.3.0#[method]tcp-socket.send",
                "wasi:sockets/types@0.3.0#[method]udp-socket.bind",
                "wasi:sockets/types@0.3.0#[method]udp-socket.connect",
                "wasi:sockets/types@0.3.0#[method]udp-socket.receive",
                "wasi:sockets/types@0.3.0#[method]udp-socket.send",
            ]
        },
        generate_all,
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
use bindings::wasi::filesystem;
use bindings::wasi::filesystem::preopens::get_directories;
use bindings::wasi::filesystem::types::Descriptor;
use bindings::wasi::filesystem::types::DescriptorFlags;
use bindings::wasi::filesystem::types::DirectoryEntry;
use bindings::wasi::filesystem::types::OpenFlags;
use bindings::wasi::filesystem::types::PathFlags;
use bindings::wasi::sockets::types::TcpSocket;
use bindings::wit_stream::StreamPayload;
use wit_bindgen::rt::async_support;
use wit_bindgen::rt::async_support::futures::SinkExt;
use wit_bindgen::rt::async_support::futures::StreamExt;
use wit_bindgen::rt::async_support::FutureReader;
use wit_bindgen::rt::async_support::StreamReader;
use wit_bindgen::rt::async_support::StreamWriter;

pub struct Session {
    last_response: String,
}

impl GuestSession for Session {
    fn new() -> Self {
        Self {
            last_response: String::new(),
        }
    }

    async fn infer(&self, request: Request) -> Response {
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

    async fn get_files_p3() -> String {
        let mut string = String::new();
        for (desc, name) in get_directories() {
            let (mut s, mut f): (
                StreamReader<DirectoryEntry>,
                FutureReader<Result<(), filesystem::types::ErrorCode>>,
            ) = desc.read_directory().await;
            for d in s.next().await.unwrap().unwrap() {
                string.push_str(&d.name);
                string.push_str("\n");
            }
        }
        string
    }
}
