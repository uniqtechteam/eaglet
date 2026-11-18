//! Serving an app over TCP. Clients that stall while sending headers are cut off, and a stop
//! signal lets the requests in flight finish before the server exits.

use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::{ConnectInfo, Request};
use hyper::body::Incoming;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tower::ServiceExt;

/// How long a client may take to send a request's headers, and how long a kept-alive connection
/// may sit idle before its next request.
pub(crate) const HEADER_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a stop waits for the requests in flight before the server exits anyway.
pub(crate) const DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

/// Accept connections on `listener` until `stop` finishes, then drain.
pub(crate) async fn serve(
    listener: TcpListener,
    router: axum::Router,
    stop: impl Future<Output = ()> + Send,
) -> std::io::Result<()> {
    let (stop_tx, stop_rx) = watch::channel(());
    let mut connections = JoinSet::new();
    let mut stop = std::pin::pin!(stop);
    loop {
        let accepted = tokio::select! {
            accepted = listener.accept() => accepted,
            () = &mut stop => break,
        };
        match accepted {
            Ok((stream, remote)) => {
                connections.spawn(connection(stream, remote, router.clone(), stop_rx.clone()));
            }
            // out of file descriptors and the like: wait instead of spinning
            Err(err) => {
                tracing::error!("accept failed: {err}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
        while connections.try_join_next().is_some() {}
    }
    drop(listener);
    stop_tx.send_replace(());
    if !connections.is_empty() {
        tracing::info!(
            "stopping: waiting for {} open connection(s)",
            connections.len()
        );
    }
    let drained = tokio::time::timeout(DRAIN_TIMEOUT, async {
        while connections.join_next().await.is_some() {}
    })
    .await;
    if drained.is_err() {
        tracing::warn!(
            "stopping: {} connection(s) still busy after {}s, closing them",
            connections.len(),
            DRAIN_TIMEOUT.as_secs()
        );
        connections.abort_all();
    }
    Ok(())
}

/// Serve one connection. Every request carries `ConnectInfo<SocketAddr>` with the peer address.
async fn connection(
    stream: TcpStream,
    remote: SocketAddr,
    router: axum::Router,
    mut stop: watch::Receiver<()>,
) {
    if let Err(err) = stream.set_nodelay(true) {
        tracing::debug!("could not set TCP_NODELAY: {err}");
    }
    let service = router.map_request(move |mut req: Request<Incoming>| {
        req.extensions_mut().insert(ConnectInfo(remote));
        req
    });
    let mut builder = hyper::server::conn::http1::Builder::new();
    builder
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_TIMEOUT);
    let conn = builder
        .serve_connection(TokioIo::new(stream), TowerToHyperService::new(service))
        .with_upgrades();
    let mut conn = std::pin::pin!(conn);
    let mut stopping = false;
    loop {
        tokio::select! {
            result = conn.as_mut() => {
                if let Err(err) = result {
                    tracing::trace!("connection from {remote} ended: {err}");
                }
                return;
            }
            _ = stop.changed(), if !stopping => {
                // finish the request in flight, then close instead of keeping the connection alive
                stopping = true;
                conn.as_mut().graceful_shutdown();
            }
        }
    }
}

/// Finishes on Ctrl-C, or on SIGTERM on Unix (what Docker, Kubernetes and systemd send).
pub(crate) async fn stop_signal() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    tracing::info!("stop signal received, finishing the requests in flight");
}
