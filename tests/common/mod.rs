use futures::stream::StreamExt;
use http_body_util::{combinators::BoxBody, BodyExt};
use hyper::service::service_fn;
use hyper_util::{
    client::legacy::{
        connect::{dns::GaiResolver, HttpConnector},
        Client,
    },
    rt::{TokioExecutor, TokioIo},
};
use std::{
    convert::Infallible,
    net::{IpAddr, SocketAddr},
};
use tokio::{net::TcpListener, sync::oneshot::Sender, task::JoinHandle};
use tokio_stream::wrappers::TcpListenerStream;
use tokiotest_httpserver::take_port;

lazy_static::lazy_static! {
    static ref PROXY_CLIENT: Client<HttpConnector<GaiResolver>, hyper::body::Incoming> = {
        Client::builder(TokioExecutor::new()).build_http()
    };
}

async fn handle(
    client_ip: IpAddr,
    req: hyper::Request<hyper::body::Incoming>,
    backend_port: u16,
) -> Result<hyper::Response<BoxBody<hyper::body::Bytes, hyper::Error>>, Infallible> {
    match hyper_reverse_proxy::call(
        client_ip,
        format!("http://127.0.0.1:{}", backend_port).as_str(),
        req,
        &*PROXY_CLIENT,
    )
    .await
    {
        Ok(response) => Ok(response.map(|body| BoxBody::new(body))),
        Err(_) => Ok(hyper::Response::builder()
            .status(502)
            .body(BoxBody::new(
                http_body_util::Empty::new().map_err(|err| match err {}),
            ))
            .unwrap()),
    }
}

pub struct SpawnServerReturn {
    pub shutdown_tx: Sender<()>,
    pub proxy_handle: JoinHandle<()>,
    pub port: u16,
}

pub async fn spawn_proxy(back_port: u16) -> SpawnServerReturn {
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let port = take_port();

    let server = TcpListenerStream::new(
        TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port)))
            .await
            .unwrap(),
    )
    .take_until(shutdown_rx)
    .for_each(move |stream| async move {
        let stream = stream.unwrap();
        let client_ip = stream.peer_addr().unwrap().ip();
        let io = TokioIo::new(stream);
        tokio::task::spawn(async move {
            if let Err(err) = hyper::server::conn::http1::Builder::new()
                .serve_connection(
                    io,
                    service_fn(|req| async { handle(client_ip, req, back_port).await }),
                )
                .with_upgrades()
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    });

    let proxy_handle = tokio::spawn(server);

    SpawnServerReturn {
        shutdown_tx,
        proxy_handle,
        port,
    }
}
