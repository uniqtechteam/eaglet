//! Procedural macros for eaglet. Use them through the `eaglet` crate.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Expr, Fields, GenericArgument, Ident, ItemStruct, PathArguments, Token, Type, bracketed,
    parse_macro_input,
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
                other => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        format!("unknown key `{other}`; expected imports, providers or exports"),
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
                }
            }
        }
    })
}
