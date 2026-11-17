//! `bench-server axum|eaglet [port]`: serve one version of the bench app on 127.0.0.1.

#![forbid(unsafe_code)]

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let which = args.next().unwrap_or_default();
    let port: u16 = args.next().and_then(|p| p.parse().ok()).unwrap_or(3000);
    let router = match which.as_str() {
        "axum" => eaglet_bench::plain::router(),
        "eaglet" => match eaglet_bench::eaglet_app::router().await {
            Ok(router) => router,
            Err(err) => {
                eprintln!("{err}");
                return ExitCode::FAILURE;
            }
        },
        _ => {
            eprintln!("usage: bench-server axum|eaglet [port]");
            return ExitCode::from(2);
        }
    };
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("cannot bind 127.0.0.1:{port}: {err}");
            return ExitCode::FAILURE;
        }
    };
    println!("{which} listening on http://127.0.0.1:{port}");
    if let Err(err) = axum::serve(listener, router).await {
        eprintln!("{err}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
