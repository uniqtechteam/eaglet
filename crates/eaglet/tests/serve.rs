//! Serving over TCP: a stop lets the requests in flight finish, and handlers see the peer address.

#![allow(clippy::unwrap_used, reason = "tests: a failure should stop the test")]
#![allow(
    clippy::unused_async,
    clippy::unused_async_trait_impl,
    clippy::unused_self,
    reason = "test handlers are written the way real controllers are, async and on &self"
)]

use std::net::SocketAddr;
use std::time::Duration;

use eaglet::axum::extract::ConnectInfo;
use eaglet::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[injectable]
struct SlowController;

#[controller("/")]
impl SlowController {
    #[get("/slow")]
    async fn slow(&self) -> &'static str {
        tokio::time::sleep(Duration::from_millis(300)).await;
        "done"
    }

    #[get("/peer")]
    async fn peer(&self, ConnectInfo(peer): ConnectInfo<SocketAddr>) -> String {
        peer.ip().to_string()
    }
}

#[module(controllers = [SlowController])]
struct AppModule;

async fn get(addr: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let req = format!("GET {path} HTTP/1.1\r\nhost: test\r\nconnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).await.unwrap();
    out
}

#[tokio::test]
async fn a_stop_lets_the_request_in_flight_finish() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let app = App::new::<AppModule>().await.unwrap();
    let server = tokio::spawn(app.serve(listener, async {
        stopped.await.ok();
    }));

    let request = tokio::spawn(get(addr, "/slow"));
    tokio::time::sleep(Duration::from_millis(100)).await;
    stop.send(()).unwrap();

    let res = request.await.unwrap();
    assert!(res.starts_with("HTTP/1.1 200"), "{res}");
    assert!(res.ends_with("done"), "{res}");
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(TcpStream::connect(addr).await.is_err(), "still accepting");
}

#[tokio::test]
async fn handlers_see_the_peer_address() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = App::new::<AppModule>().await.unwrap();
    tokio::spawn(app.serve(listener, std::future::pending()));
    let res = get(addr, "/peer").await;
    assert!(res.ends_with("127.0.0.1"), "{res}");
}
