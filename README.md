# eaglet

eaglet brings the structure people like in NestJS to Rust web apps: modules, dependency injection,
controllers, guards, validation, exception filters, testing modules and a generator CLI. It runs on plain
Axum underneath, so every Axum and Tower tool keeps working.

**Status:** early development, not released yet.

## The idea in one example

This is the shape eaglet is being built towards:

```rust
#[injectable]
struct UsersService {
    repo: Arc<UsersRepo>,
}

#[controller("/users")]
impl UsersController {
    #[get("/:id")]
    async fn find(&self, Path(id): Path<i64>) -> Result<Json<User>, HttpError> {
        self.users.find(id).await
    }
}

#[module(controllers = [UsersController], providers = [UsersService, UsersRepo])]
struct UsersModule;
```

## Licence

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.

---

Built by [UniqTech](https://uniqtech.dev).
