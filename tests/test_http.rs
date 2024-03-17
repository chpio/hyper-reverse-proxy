mod common;

use hyper014::{
    header::{CONNECTION, UPGRADE},
    Body, Request, StatusCode, Uri,
};
use test_context::{test_context, AsyncTestContext};
use tokio::{sync::oneshot::Sender, task::JoinHandle};
use tokiotest_httpserver::{handler::HandlerBuilder, HttpTestContext};

#[test_context(ProxyTestContext)]
#[tokio::test]
async fn test_get_error_500(ctx: &mut ProxyTestContext) {
    let client = hyper014::Client::new();
    let resp = client
        .request(
            Request::builder()
                .header("keep-alive", "treu")
                .method("GET")
                .uri(ctx.uri("/500"))
                .body(Body::from(""))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(500, resp.status());
}

#[test_context(ProxyTestContext)]
#[tokio::test]
async fn test_upgrade_mismatch(ctx: &mut ProxyTestContext) {
    ctx.http_back.add(
        HandlerBuilder::new("/ws")
            .status_code(StatusCode::SWITCHING_PROTOCOLS)
            .build(),
    );
    let resp = hyper014::Client::new()
        .request(
            Request::builder()
                .header(CONNECTION, "Upgrade")
                .header(UPGRADE, "websocket")
                .method("GET")
                .uri(ctx.uri("/ws"))
                .body(Body::from(""))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 502);
}

#[test_context(ProxyTestContext)]
#[tokio::test]
async fn test_upgrade_unrequested(ctx: &mut ProxyTestContext) {
    ctx.http_back.add(
        HandlerBuilder::new("/wrong_switch")
            .status_code(StatusCode::SWITCHING_PROTOCOLS)
            .build(),
    );
    let resp = hyper014::Client::new()
        .get(ctx.uri("/wrong_switch"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 502);
}

#[test_context(ProxyTestContext)]
#[tokio::test]
async fn test_get(ctx: &mut ProxyTestContext) {
    ctx.http_back.add(
        HandlerBuilder::new("/foo")
            .status_code(StatusCode::OK)
            .build(),
    );
    let resp = hyper014::Client::new().get(ctx.uri("/foo")).await.unwrap();
    assert_eq!(200, resp.status());
}

struct ProxyTestContext {
    http_back: HttpTestContext,
    shutdown_tx: Sender<()>,
    proxy_handle: JoinHandle<()>,
    port: u16,
}

#[async_trait::async_trait]
impl<'a> AsyncTestContext for ProxyTestContext {
    async fn setup() -> ProxyTestContext {
        let http_back: HttpTestContext = AsyncTestContext::setup().await;

        let proxy = common::spawn_proxy(http_back.port).await;

        ProxyTestContext {
            shutdown_tx: proxy.shutdown_tx,
            proxy_handle: proxy.proxy_handle,
            port: proxy.port,
            http_back,
        }
    }
    async fn teardown(self) {
        let _ = AsyncTestContext::teardown(self.http_back);
        let _ = self.shutdown_tx.send(()).unwrap();
        let _ = tokio::join!(self.proxy_handle);
    }
}
impl ProxyTestContext {
    pub fn uri(&self, path: &str) -> Uri {
        format!("http://{}:{}{}", "localhost", self.port, path)
            .parse::<Uri>()
            .unwrap()
    }
}
