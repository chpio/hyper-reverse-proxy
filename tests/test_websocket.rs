mod common;

use async_tungstenite::tokio::{accept_async, connect_async};
use futures::{SinkExt, StreamExt};
use std::{process::exit, time::Duration};
use test_context::{test_context, AsyncTestContext};
use tokio::{net::TcpListener, sync::oneshot::Sender, task::JoinHandle};
use tokiotest_httpserver::take_port;
use tungstenite::Message;
use url::Url;

#[test_context(ProxyTestContext)]
#[tokio::test]
async fn test_websocket(ctx: &mut ProxyTestContext) {
    let (mut client, _) =
        connect_async(Url::parse(&format!("ws://127.0.0.1:{}", ctx.port)).unwrap())
            .await
            .unwrap();

    client.send(Message::Ping("hello".into())).await.unwrap();
    let msg = client.next().await.unwrap().unwrap();

    assert!(
        matches!(&msg, Message::Pong(inner) if inner == "hello".as_bytes()),
        "did not get pong, but {:?}",
        msg
    );

    let msg = client.next().await.unwrap().unwrap();

    assert!(
        matches!(&msg, Message::Text(inner) if inner == "All done"),
        "did not get text, but {:?}",
        msg
    );
}

struct ProxyTestContext {
    ws_handle: JoinHandle<()>,
    shutdown_tx: Sender<()>,
    proxy_handle: JoinHandle<()>,
    port: u16,
}

#[async_trait::async_trait]
impl<'a> AsyncTestContext for ProxyTestContext {
    async fn setup() -> ProxyTestContext {
        tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            println!("Unit test executed too long, perhaps its stuck...");
            exit(1);
        });

        let ws_port = take_port();

        let ws_handle = tokio::spawn(async move {
            let ws_server = TcpListener::bind(("127.0.0.1", ws_port)).await.unwrap();

            if let Ok((stream, _)) = ws_server.accept().await {
                let mut websocket = accept_async(stream).await.unwrap();

                let msg = websocket.next().await.unwrap().unwrap();
                assert!(
                    matches!(&msg, Message::Ping(inner) if inner == "hello".as_bytes()),
                    "did not get ping, but: {:?}",
                    msg
                );

                // Tungstenite will auto send a Pong as a response to a Ping, but we still need
                // to flush
                websocket.flush().await.unwrap();

                websocket
                    .send(Message::Text("All done".to_string()))
                    .await
                    .unwrap();
            }
        });

        let proxy = common::spawn_proxy(ws_port).await;

        ProxyTestContext {
            shutdown_tx: proxy.shutdown_tx,
            proxy_handle: proxy.proxy_handle,
            port: proxy.port,
            ws_handle,
        }
    }
    async fn teardown(self) {
        let _ = self.shutdown_tx.send(()).unwrap();
        let _ = tokio::join!(self.proxy_handle, self.ws_handle);
    }
}
