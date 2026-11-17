//! `bench-inproc [requests]`: call both routers in memory, with no sockets, and print the median
//! nanoseconds per request for every case as JSON lines. This is the framework's own cost per
//! request, without the network stack and the load generator in the picture.

#![forbid(unsafe_code)]

use std::time::Instant;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, header};
use tower::ServiceExt;

const SAMPLES: usize = 7;

fn request(case: &str) -> Request<Body> {
    let builder = match case {
        "get" => Request::get("/users/42"),
        "post" => Request::post("/users").header(header::CONTENT_TYPE, "application/json"),
        _ => Request::get("/me").header(header::AUTHORIZATION, "Bearer bench-token"),
    };
    let body = if case == "post" {
        Body::from(r#"{"name":"Ana","email":"ana@example.com"}"#)
    } else {
        Body::empty()
    };
    builder.body(body).unwrap_or_default()
}

async fn one(router: &Router, case: &str) -> usize {
    let Ok(res) = router.clone().oneshot(request(case)).await;
    assert!(res.status().is_success(), "{case}: {}", res.status());
    to_bytes(res.into_body(), 4096).await.map_or(0, |b| b.len())
}

async fn ns_per_request(router: &Router, case: &str, requests: u32) -> f64 {
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        let mut bytes = 0;
        for _ in 0..requests {
            bytes += one(router, case).await;
        }
        std::hint::black_box(bytes);
        samples.push(start.elapsed().as_secs_f64() * 1e9 / f64::from(requests));
    }
    samples.sort_by(f64::total_cmp);
    samples[SAMPLES / 2]
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let requests: u32 = std::env::args()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(200_000);
    let routers = [
        ("axum", eaglet_bench::plain::router()),
        ("eaglet", eaglet_bench::eaglet_app::router().await?),
    ];
    for case in ["get", "post", "me"] {
        for (name, router) in &routers {
            ns_per_request(router, case, requests / 10).await; // warm up
            let ns = ns_per_request(router, case, requests).await;
            println!(r#"{{"case":"{case}","framework":"{name}","ns":{ns:.0}}}"#);
        }
    }
    Ok(())
}
