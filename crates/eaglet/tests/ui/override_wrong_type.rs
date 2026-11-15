use eaglet::prelude::*;
use eaglet::testing::TestApp;

trait UsersRepo: Send + Sync {}

#[module(providers = [])]
struct AppModule;

fn main() {
    let _ = TestApp::new::<AppModule>().override_provider::<dyn UsersRepo>(Arc::new(42_u32));
}
