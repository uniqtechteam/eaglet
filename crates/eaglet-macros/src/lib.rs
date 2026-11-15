//! Procedural macros for eaglet. Use them through the `eaglet` crate.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Expr, Fields, FnArg, GenericArgument, Ident, ImplItem, ItemImpl, ItemStruct, LitStr,
    PathArguments, Token, Type, bracketed, parse_macro_input,
};

/// Make a struct a provider. Its fields are other providers held as `Arc<T>` (or `Arc<dyn Trait>`);
/// eaglet builds them first and passes them in. Mark a field `#[inject(default)]` to fill it with
/// `Default::default()` instead.
///
/// ```ignore
/// #[injectable]
/// struct UsersService {
///     repo: Arc<dyn UsersRepo>,
/// }
/// ```
#[proc_macro_attribute]
pub fn injectable(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[injectable] takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    let item = parse_macro_input!(item as ItemStruct);
    expand_injectable(item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// `Arc<T>` → `T`.
fn arc_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else { return None };
    let last = path.path.segments.last()?;
    if last.ident != "Arc" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    match args.args.first()? {
        GenericArgument::Type(inner) if args.args.len() == 1 => Some(inner),
        _ => None,
    }
}

fn expand_injectable(mut item: ItemStruct) -> syn::Result<TokenStream2> {
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "generic providers are not supported yet",
        ));
    }
    let name = item.ident.clone();
    let mut deps = Vec::new();
    let mut inits = Vec::new();
    match &mut item.fields {
        Fields::Unit => {}
        Fields::Unnamed(f) => {
            return Err(syn::Error::new_spanned(
                f,
                "#[injectable] needs named fields, for example `repo: Arc<UsersRepo>`",
            ));
        }
        Fields::Named(fields) => {
            for field in &mut fields.named {
                let ident = field.ident.clone();
                let wants_default = field.attrs.iter().any(|a| {
                    a.path().is_ident("inject")
                        && a.parse_args::<Ident>().is_ok_and(|i| i == "default")
                });
                field.attrs.retain(|a| !a.path().is_ident("inject"));
                if wants_default {
                    inits.push(quote! { #ident: ::core::default::Default::default() });
                    continue;
                }
                let Some(inner) = arc_inner(&field.ty) else {
                    return Err(syn::Error::new_spanned(
                        &field.ty,
                        "a provider's fields must be `Arc<SomeProvider>` (or mark the field #[inject(default)])",
                    ));
                };
                deps.push(quote! { ::eaglet::Dep::of::<#inner>() });
                inits.push(quote! { #ident: container.get::<#inner>()? });
            }
        }
    }
    let body = if matches!(item.fields, Fields::Unit) {
        quote! { Self }
    } else {
        quote! { Self { #(#inits),* } }
    };
    Ok(quote! {
        #item

        impl ::eaglet::Injectable for #name {
            fn deps() -> ::std::vec::Vec<::eaglet::Dep> {
                ::std::vec![#(#deps),*]
            }

            fn build(container: &::eaglet::Container) -> ::core::result::Result<Self, ::eaglet::StartupError> {
                ::core::result::Result::Ok(#body)
            }
        }
    })
}

/// The arguments of `#[module(...)]`.
#[derive(Default)]
struct ModuleArgs {
    imports: Vec<Type>,
    providers: Vec<Expr>,
    controllers: Vec<Type>,
    exports: Vec<Type>,
}

impl Parse for ModuleArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut args = Self::default();
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let content;
            bracketed!(content in input);
            match key.to_string().as_str() {
                "imports" => {
                    args.imports = Punctuated::<Type, Token![,]>::parse_terminated(&content)?
                        .into_iter()
                        .collect();
                }
                "providers" => {
                    args.providers = Punctuated::<Expr, Token![,]>::parse_terminated(&content)?
                        .into_iter()
                        .collect();
                }
                "exports" => {
                    args.exports = Punctuated::<Type, Token![,]>::parse_terminated(&content)?
                        .into_iter()
                        .collect();
                }
                "controllers" => {
                    args.controllers = Punctuated::<Type, Token![,]>::parse_terminated(&content)?
                        .into_iter()
                        .collect();
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        format!(
                            "unknown key `{other}`; expected imports, controllers, providers or exports"
                        ),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(args)
    }
}

/// Declare a module on a unit struct: what it imports, which providers it declares and which of them
/// other modules may use.
///
/// ```ignore
/// #[module(imports = [DbModule], providers = [UsersService, provide!(dyn UsersRepo => PgUsersRepo)], exports = [UsersService])]
/// struct UsersModule;
/// ```
#[proc_macro_attribute]
pub fn module(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as ModuleArgs);
    let item = parse_macro_input!(item as ItemStruct);
    expand_module(&args, &item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_module(args: &ModuleArgs, item: &ItemStruct) -> syn::Result<TokenStream2> {
    if !matches!(item.fields, Fields::Unit) {
        return Err(syn::Error::new_spanned(
            &item.ident,
            "a module is a unit struct, for example `struct UsersModule;`",
        ));
    }
    let name = &item.ident;
    let name_str = name.to_string();
    let imports = &args.imports;
    let providers = args.providers.iter().map(|p| match p {
        Expr::Path(path) => quote! { ::eaglet::ProviderDef::of::<#path>() },
        other => quote! { #other },
    });
    let exports = &args.exports;
    let controllers = &args.controllers;
    Ok(quote! {
        #item

        impl ::eaglet::Module for #name {
            fn definition() -> ::eaglet::ModuleDef {
                ::eaglet::ModuleDef {
                    id: ::std::any::TypeId::of::<Self>(),
                    name: #name_str,
                    imports: ::std::vec![#(<#imports as ::eaglet::Module>::definition as fn() -> ::eaglet::ModuleDef),*],
                    providers: ::std::vec![#(#providers),*],
                    exports: ::std::vec![#(::eaglet::Dep::of::<#exports>()),*],
                    controllers: ::std::vec![#(::eaglet::ControllerDef::of::<#controllers>()),*],
                }
            }
        }
    })
}

const ROUTE_ATTRS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

/// Turn `/users` + `/:id/*rest` into Axum's `/users/{id}/{*rest}`.
fn route_path(prefix: &str, path: &str) -> String {
    let joined = format!(
        "{}/{}",
        prefix.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let segments: Vec<String> = joined
        .split('/')
        .map(|seg| {
            if let Some(name) = seg.strip_prefix(':') {
                format!("{{{name}}}")
            } else if let Some(name) = seg.strip_prefix('*') {
                format!("{{*{name}}}")
            } else {
                seg.to_owned()
            }
        })
        .collect();
    let path = segments.join("/");
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        "/".to_owned()
    } else {
        path.to_owned()
    }
}

/// One route found in a controller.
struct Route {
    method: Ident,
    path: String,
    handler: TokenStream2,
}

/// Mark an `impl` block as a controller and give it a path prefix. Its methods with `#[get]`,
/// `#[post]`, `#[put]`, `#[patch]` or `#[delete]` become routes; they take `&self` and any Axum
/// extractors. The struct itself must be `#[injectable]`.
///
/// ```ignore
/// #[controller("/users")]
/// impl UsersController {
///     #[get("/:id")]
///     async fn find(&self, Path(id): Path<u64>) -> Json<User> { ... }
/// }
/// ```
#[proc_macro_attribute]
pub fn controller(attr: TokenStream, item: TokenStream) -> TokenStream {
    let prefix = parse_macro_input!(attr as LitStr);
    let item = parse_macro_input!(item as ItemImpl);
    expand_controller(&prefix, item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand_controller(prefix: &LitStr, mut item: ItemImpl) -> syn::Result<TokenStream2> {
    if let Some((_, path, _)) = &item.trait_ {
        return Err(syn::Error::new_spanned(
            path,
            "#[controller] goes on a plain `impl` block, not a trait impl",
        ));
    }
    let self_ty = item.self_ty.clone();
    let mut routes: Vec<Route> = Vec::new();
    for impl_item in &mut item.items {
        let ImplItem::Fn(method) = impl_item else {
            continue;
        };
        let Some(pos) = method
            .attrs
            .iter()
            .position(|a| ROUTE_ATTRS.iter().any(|r| a.path().is_ident(r)))
        else {
            continue;
        };
        let attr = method.attrs.remove(pos);
        let verb = attr
            .path()
            .get_ident()
            .cloned()
            .ok_or_else(|| syn::Error::new_spanned(&attr, "expected a route attribute"))?;
        let path: LitStr = attr.parse_args().map_err(|_| {
            syn::Error::new_spanned(
                &attr,
                format!("write the path as a string, for example #[{verb}(\"/:id\")]"),
            )
        })?;
        let mut inputs = method.sig.inputs.iter();
        match inputs.next() {
            Some(FnArg::Receiver(r)) if r.reference.is_some() && r.mutability.is_none() => {}
            _ => {
                return Err(syn::Error::new_spanned(
                    &method.sig,
                    "route handlers take `&self` first, then Axum extractors",
                ));
            }
        }
        let mut names = Vec::new();
        let mut arg_types = Vec::new();
        for (i, arg) in inputs.enumerate() {
            let FnArg::Typed(typed) = arg else { continue };
            names.push(quote::format_ident!("__eaglet_arg{}", i));
            arg_types.push(typed.ty.clone());
        }
        let name = &method.sig.ident;
        let call = if method.sig.asyncness.is_some() {
            quote! { this.#name(#(#names),*).await }
        } else {
            quote! { this.#name(#(#names),*) }
        };
        let handler = quote! {{
            let this = ::std::sync::Arc::clone(&this);
            move |#(#names: #arg_types),*| {
                let this = ::std::sync::Arc::clone(&this);
                async move { #call }
            }
        }};
        routes.push(Route {
            method: verb,
            path: route_path(&prefix.value(), &path.value()),
            handler,
        });
    }
    // one method router per path: get(a).post(b)
    let mut paths: Vec<String> = Vec::new();
    for r in &routes {
        if !paths.contains(&r.path) {
            paths.push(r.path.clone());
        }
    }
    let mut route_calls = Vec::new();
    for path in &paths {
        let mut method_router: Option<TokenStream2> = None;
        for r in routes.iter().filter(|r| &r.path == path) {
            let verb = &r.method;
            let handler = &r.handler;
            method_router = Some(match method_router {
                None => quote! { ::eaglet::axum::routing::#verb(#handler) },
                Some(mr) => quote! { #mr.#verb(#handler) },
            });
        }
        route_calls.push(quote! { .route(#path, #method_router) });
    }
    Ok(quote! {
        #item

        impl ::eaglet::Controller for #self_ty {
            fn router(container: &::eaglet::Container) -> ::core::result::Result<::eaglet::axum::Router, ::eaglet::StartupError> {
                let this = container.get::<Self>()?;
                ::core::result::Result::Ok(::eaglet::axum::Router::new() #(#route_calls)*)
            }
        }
    })
}

fn outside_controller(verb: &str) -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        format!("#[{verb}] only works on a method inside an `impl` block marked #[controller(\"/prefix\")]"),
    )
    .to_compile_error()
    .into()
}

/// A `GET` route inside a `#[controller]` impl: `#[get("/:id")]`.
#[proc_macro_attribute]
pub fn get(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    outside_controller("get")
}

/// A `POST` route inside a `#[controller]` impl.
#[proc_macro_attribute]
pub fn post(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    outside_controller("post")
}

/// A `PUT` route inside a `#[controller]` impl.
#[proc_macro_attribute]
pub fn put(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    outside_controller("put")
}

/// A `PATCH` route inside a `#[controller]` impl.
#[proc_macro_attribute]
pub fn patch(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    outside_controller("patch")
}

/// A `DELETE` route inside a `#[controller]` impl.
#[proc_macro_attribute]
pub fn delete(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    outside_controller("delete")
}

#[cfg(test)]
mod tests {
    use super::route_path;

    #[test]
    fn nest_style_paths_become_axum_paths() {
        assert_eq!(route_path("/users", "/:id"), "/users/{id}");
        assert_eq!(route_path("/users", "/"), "/users");
        assert_eq!(route_path("/", "/"), "/");
        assert_eq!(route_path("/files", "*rest"), "/files/{*rest}");
        assert_eq!(route_path("/a/", "/b/:x/c"), "/a/b/{x}/c");
    }
}
