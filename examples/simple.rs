use futures::stream::StreamExt;
use http_body_util::{combinators::BoxBody, BodyExt};
use hyper::{service::service_fn, StatusCode};

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
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

lazy_static::lazy_static! {
    static ref PROXY_CLIENT: Client<HttpConnector<GaiResolver>, hyper::body::Incoming> = {
        Client::builder(TokioExecutor::new()).build_http()
    };
}

type Request = hyper::Request<hyper::body::Incoming>;
type Response = hyper::Response<BoxBody<hyper::body::Bytes, hyper::Error>>;

fn debug_request(req: &Request) -> Result<Response, Infallible> {
    // TODO: await full request body?
    let body_str = format!("{:?}", req);
    Ok(Response::new(
        http_body_util::Full::new(body_str.into_bytes().into())
            .map_err(|err| match err {})
            .boxed(),
    ))
}

async fn handle(client_ip: IpAddr, req: Request) -> Result<Response, Infallible> {
    if req.uri().path().starts_with("/target/first") {
        match hyper_reverse_proxy::call(client_ip, "http://127.0.0.1:13901", req, &*PROXY_CLIENT)
            .await
        {
            Ok(response) => Ok(response.map(|body| body.boxed())),
            Err(_error) => Ok(hyper::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(
                    http_body_util::Empty::new()
                        .map_err(|err| match err {})
                        .boxed(),
                )
                .unwrap()),
        }
    } else if req.uri().path().starts_with("/target/second") {
        match hyper_reverse_proxy::call(client_ip, "http://127.0.0.1:13902", req, &*PROXY_CLIENT)
            .await
        {
            Ok(response) => Ok(response.map(|body| body.boxed())),
            Err(_error) => Ok(hyper::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(
                    http_body_util::Empty::new()
                        .map_err(|err| match err {})
                        .boxed(),
                )
                .unwrap()),
        }
    } else {
        debug_request(&req)
    }
}

#[tokio::main]
async fn main() {
    // let bind_addr = "127.0.0.1:8000";
    // let addr: SocketAddr = bind_addr.parse().expect("Could not parse ip:port.");

    // let make_svc = make_service_fn(|conn: &AddrStream| {
    //     let remote_addr = conn.remote_addr().ip();
    //     async move { Ok::<_, Infallible>(service_fn(move |req| handle(remote_addr, req))) }
    // });

    // let server = Server::bind(&addr).serve(make_svc);

    let addr = SocketAddr::from(([127, 0, 0, 1], 8000));
    let server = TcpListenerStream::new(
        TcpListener::bind(addr)
            .await
            .expect("Could not parse ip:port."),
    )
    .for_each(move |stream| async move {
        let stream = stream.unwrap();
        let client_ip = stream.peer_addr().unwrap().ip();
        let io = TokioIo::new(stream);
        tokio::task::spawn(async move {
            if let Err(err) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service_fn(|req| async { handle(client_ip, req).await }))
                .with_upgrades()
                .await
            {
                println!("Error serving connection: {:?}", err);
            }
        });
    });

    println!("Running server on {:?}", addr);

    server.await;
}
