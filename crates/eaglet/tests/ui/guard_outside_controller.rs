use eaglet::prelude::*;

#[injectable]
struct AuthGuard;

struct Thing;

#[guard(AuthGuard)]
impl Thing {}

fn main() {}
