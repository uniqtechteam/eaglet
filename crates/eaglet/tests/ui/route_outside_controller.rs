use eaglet::prelude::*;

#[get("/")]
async fn hello() -> &'static str {
    "hi"
}

fn main() {}
