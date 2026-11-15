//! `#[controller]`: an `impl` block becomes an Axum router, with guards and layers.

use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::{Attribute, Expr, FnArg, Ident, ImplItem, ImplItemFn, ItemImpl, LitStr, Token, Type};

use crate::doc::{RouteInfo, route_doc};

const ROUTE_ATTRS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

/// Turn `/users` + `/:id/*rest` into Axum's `/users/{id}/{*rest}`.
pub fn route_path(prefix: &str, path: &str) -> String {
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

/// `#[guard(..)]` and `#[layer(..)]` found on a controller or a route, in the order written.
#[derive(Default)]
struct Wrappers {
    guards: Vec<Type>,
    layers: Vec<Expr>,
}

impl Wrappers {
    /// Take the `#[guard]` and `#[layer]` attributes out of `attrs`.
    fn take(attrs: &mut Vec<Attribute>) -> syn::Result<Self> {
        let mut found = Self::default();
        let mut kept = Vec::new();
        for attr in attrs.drain(..) {
            if attr.path().is_ident("guard") {
                let list = attr
                    .parse_args_with(Punctuated::<Type, Token![,]>::parse_terminated)
                    .map_err(|_| {
                        syn::Error::new_spanned(
                            &attr,
                            "name the guards, for example #[guard(AuthGuard)]",
                        )
                    })?;
                found.guards.extend(list);
            } else if attr.path().is_ident("layer") {
                let layer = attr.parse_args::<Expr>().map_err(|_| {
                    syn::Error::new_spanned(
                        &attr,
                        "give one Tower layer, for example #[layer(TimeoutLayer::new(..))]",
                    )
                })?;
                found.layers.push(layer);
            } else {
                kept.push(attr);
            }
        }
        *attrs = kept;
        Ok(found)
    }

    fn is_empty(&self) -> bool {
        self.guards.is_empty() && self.layers.is_empty()
    }

    /// Wrap `target` so the first guard written runs first, and layers run outside the guards.
    fn apply(&self, target: TokenStream2, guard_fn: &TokenStream2) -> TokenStream2 {
        let mut out = target;
        for guard in self.guards.iter().rev() {
            out = quote! { #guard_fn(#out, container.get::<#guard>()?) };
        }
        for layer in self.layers.iter().rev() {
            out = quote! { (#out).route_layer(#layer) };
        }
        out
    }
}

/// One route found in a controller.
struct Route {
    verb: Ident,
    path: String,
    handler: TokenStream2,
    wrappers: Wrappers,
    doc: TokenStream2,
}

fn route_attr_pos(attrs: &[Attribute]) -> Option<usize> {
    attrs
        .iter()
        .position(|a| ROUTE_ATTRS.iter().any(|r| a.path().is_ident(r)))
}

/// The route declared on `method`, if it has a route attribute.
fn take_route(
    prefix: &LitStr,
    controller: &str,
    method: &mut ImplItemFn,
) -> syn::Result<Option<Route>> {
    let Some(pos) = route_attr_pos(&method.attrs) else {
        let wrappers = Wrappers::take(&mut method.attrs)?;
        if !wrappers.is_empty() {
            return Err(syn::Error::new_spanned(
                &method.sig.ident,
                "#[guard] and #[layer] on a method need a route attribute too, for example #[get(\"/\")]",
            ));
        }
        return Ok(None);
    };
    let attr = method.attrs.remove(pos);
    if let Some(extra) = route_attr_pos(&method.attrs) {
        return Err(syn::Error::new_spanned(
            &method.attrs[extra],
            "a method serves one route; write a second method for the other one",
        ));
    }
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
    let wrappers = Wrappers::take(&mut method.attrs)?;
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
        names.push(format_ident!("__eaglet_arg{}", i));
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
    let path = route_path(&prefix.value(), &path.value());
    let doc = route_doc(&RouteInfo {
        verb: &verb,
        path: &path,
        method: name,
        controller,
        attrs: &method.attrs,
        args: &arg_types,
        output: &method.sig.output,
    });
    Ok(Some(Route {
        verb,
        path,
        handler,
        wrappers,
        doc,
    }))
}

/// `.route(path, get(a).merge(post(b)))` for every path, each method wrapped in its own guards and
/// layers.
fn route_calls(routes: &[Route]) -> syn::Result<Vec<TokenStream2>> {
    let guard_method = quote! { ::eaglet::__private::guard_method };
    let mut paths: Vec<&str> = Vec::new();
    for r in routes {
        if !paths.contains(&r.path.as_str()) {
            paths.push(&r.path);
        }
    }
    let mut calls = Vec::new();
    for path in paths {
        let mut seen: Vec<String> = Vec::new();
        let mut method_router: Option<TokenStream2> = None;
        for r in routes.iter().filter(|r| r.path == path) {
            let verb = &r.verb;
            if seen.contains(&verb.to_string()) {
                return Err(syn::Error::new_spanned(
                    verb,
                    format!(
                        "two methods serve {} {path}",
                        verb.to_string().to_uppercase()
                    ),
                ));
            }
            seen.push(verb.to_string());
            let handler = &r.handler;
            let one = r.wrappers.apply(
                quote! { ::eaglet::axum::routing::#verb(#handler) },
                &guard_method,
            );
            method_router = Some(match method_router {
                None => one,
                Some(acc) => quote! { (#acc).merge(#one) },
            });
        }
        calls.push(quote! { .route(#path, #method_router) });
    }
    Ok(calls)
}

pub fn expand(prefix: &LitStr, mut item: ItemImpl) -> syn::Result<TokenStream2> {
    if let Some((_, path, _)) = &item.trait_ {
        return Err(syn::Error::new_spanned(
            path,
            "#[controller] goes on a plain `impl` block, not a trait impl",
        ));
    }
    let controller = Wrappers::take(&mut item.attrs)?;
    let self_ty = item.self_ty.clone();
    let controller_name = match &*self_ty {
        Type::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    };
    let mut routes: Vec<Route> = Vec::new();
    for impl_item in &mut item.items {
        let ImplItem::Fn(method) = impl_item else {
            continue;
        };
        if let Some(route) = take_route(prefix, &controller_name, method)? {
            routes.push(route);
        }
    }
    let guards: Vec<&Type> = controller
        .guards
        .iter()
        .chain(routes.iter().flat_map(|r| &r.wrappers.guards))
        .collect();
    let calls = route_calls(&routes)?;
    let docs: Vec<&TokenStream2> = routes.iter().map(|r| &r.doc).collect();
    let body = quote! { ::eaglet::axum::Router::new() #(#calls)* };
    // a router without routes cannot take route layers; its guards are still checked at startup
    let body = if routes.is_empty() {
        body
    } else {
        controller.apply(body, &quote! { ::eaglet::__private::guard_router })
    };
    Ok(quote! {
        #item

        impl ::eaglet::Controller for #self_ty {
            fn router(container: &::eaglet::Container) -> ::core::result::Result<::eaglet::axum::Router, ::eaglet::StartupError> {
                let this = container.get::<Self>()?;
                ::core::result::Result::Ok(#body)
            }

            fn route_deps() -> ::std::vec::Vec<::eaglet::Dep> {
                ::std::vec![#(::eaglet::Dep::of::<#guards>()),*]
            }

            fn route_docs() -> ::std::vec::Vec<::eaglet::__private::doc::RouteDoc> {
                ::std::vec![#(#docs),*]
            }
        }
    })
}
