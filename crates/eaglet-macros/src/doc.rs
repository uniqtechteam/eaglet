//! What the API docs say about a route, read from its signature: path parameters, `Query<T>`,
//! `Json<T>` bodies, the return type and the doc comment.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Attribute, Expr, ExprLit, GenericArgument, Ident, Lit, Meta, PathArguments, ReturnType, Type,
};

/// What the macro knows about one route.
pub struct RouteInfo<'a> {
    pub verb: &'a Ident,
    pub path: &'a str,
    pub method: &'a Ident,
    pub controller: &'a str,
    pub attrs: &'a [Attribute],
    pub args: &'a [Box<Type>],
    pub output: &'a ReturnType,
}

/// `Json<T>` → `("Json", [T])`.
fn last_segment(ty: &Type) -> Option<(String, Vec<&Type>)> {
    let Type::Path(path) = ty else { return None };
    let last = path.path.segments.last()?;
    let args = match &last.arguments {
        PathArguments::AngleBracketed(a) => a
            .args
            .iter()
            .filter_map(|g| match g {
                GenericArgument::Type(t) => Some(t),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Some((last.ident.to_string(), args))
}

/// `Valid<Json<T>>` and `Json<T>` both give `("Json", T)`.
fn extractor(ty: &Type) -> Option<(String, &Type)> {
    let (name, args) = last_segment(ty)?;
    let first = *args.first()?;
    if name == "Valid" {
        return extractor(first);
    }
    Some((name, first))
}

fn probe(ty: &Type) -> TokenStream2 {
    quote! {{
        #[allow(unused_imports)]
        use ::eaglet::__private::doc::{ViaAny as _, ViaToSchema as _};
        (&::eaglet::__private::doc::Probe::<#ty>::new()).schema_doc()
    }}
}

fn some(tokens: Option<TokenStream2>) -> TokenStream2 {
    tokens.map_or_else(
        || quote! { ::core::option::Option::None },
        |t| quote! { ::core::option::Option::Some(#t) },
    )
}

/// The doc comment, one line per `///` line.
fn doc_text(attrs: &[Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if let Meta::NameValue(nv) = &attr.meta
            && nv.path.is_ident("doc")
            && let Expr::Lit(ExprLit {
                lit: Lit::Str(s), ..
            }) = &nv.value
        {
            lines.push(s.value().trim().to_owned());
        }
    }
    lines.join("\n")
}

/// `{id}` and `{*rest}` names in an Axum path.
fn param_names(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|seg| seg.strip_prefix('{')?.strip_suffix('}'))
        .map(|name| name.trim_start_matches('*').to_owned())
        .collect()
}

fn path_params(path: &str, path_ty: Option<&Type>) -> Vec<TokenStream2> {
    let names = param_names(path);
    let schemas: Vec<TokenStream2> = match path_ty {
        Some(Type::Tuple(tuple)) if tuple.elems.len() == names.len() => {
            tuple.elems.iter().map(probe).collect()
        }
        Some(ty) if names.len() == 1 => vec![probe(ty)],
        Some(ty) => {
            let whole = probe(ty);
            names
                .iter()
                .map(|n| quote! { ::eaglet::__private::doc::SchemaDoc::property(&#whole, #n) })
                .collect()
        }
        None => names
            .iter()
            .map(|_| quote! { ::eaglet::__private::doc::SchemaDoc::string() })
            .collect(),
    };
    names
        .iter()
        .zip(schemas)
        .map(|(n, s)| quote! { (#n, #s) })
        .collect()
}

/// `(status, content type, schema)` for the success response.
fn response(output: &ReturnType) -> (&'static str, Option<&'static str>, Option<TokenStream2>) {
    let ReturnType::Type(_, ty) = output else {
        return ("200", None, None);
    };
    let mut ty: &Type = ty;
    if let Some((name, args)) = last_segment(ty)
        && name == "Result"
        && let Some(ok) = args.first()
    {
        ty = ok;
    }
    let mut status = "200";
    if let Type::Tuple(tuple) = ty {
        status = "2XX";
        match tuple.elems.last() {
            Some(last) => ty = last,
            None => return ("200", None, None),
        }
    }
    if let Type::Reference(r) = ty
        && matches!(&*r.elem, Type::Path(p) if p.path.is_ident("str"))
    {
        return (status, Some("text/plain"), None);
    }
    match last_segment(ty) {
        Some((name, args)) if name == "Json" => (
            status,
            Some("application/json"),
            args.first().map(|t| probe(t)),
        ),
        Some((name, _)) if name == "String" => (status, Some("text/plain"), None),
        Some((name, _)) if name == "Html" => (status, Some("text/html"), None),
        Some((name, _)) if name == "StatusCode" => ("2XX", None, None),
        _ => (status, None, None),
    }
}

/// The `RouteDoc` for one route.
pub fn route_doc(info: &RouteInfo<'_>) -> TokenStream2 {
    let mut body = None;
    let mut query = None;
    let mut path_ty = None;
    for ty in info.args {
        match extractor(ty) {
            Some((name, inner)) if name == "Json" => body = Some(probe(inner)),
            Some((name, inner)) if name == "Query" => query = Some(probe(inner)),
            Some((name, inner)) if name == "Path" => path_ty = Some(inner),
            _ => {}
        }
    }
    let method = info.verb.to_string();
    let path = info.path;
    let operation_id = format!("{}_{}", info.controller, info.method);
    let tag = info
        .controller
        .strip_suffix("Controller")
        .filter(|t| !t.is_empty())
        .unwrap_or(info.controller);
    let docs = doc_text(info.attrs);
    let params = path_params(info.path, path_ty);
    let query = some(query);
    let body = some(body);
    let (status, content_type, schema) = response(info.output);
    let content_type = some(content_type.map(|c| quote! { #c }));
    let schema = some(schema);
    quote! {
        ::eaglet::__private::doc::RouteDoc {
            method: #method,
            path: #path,
            operation_id: #operation_id,
            tag: #tag,
            docs: #docs,
            path_params: ::std::vec![#(#params),*],
            query: #query,
            body: #body,
            response: ::eaglet::__private::doc::ResponseDoc {
                status: #status,
                content_type: #content_type,
                schema: #schema,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::param_names;

    #[test]
    fn finds_path_parameters() {
        assert_eq!(param_names("/users/{id}/files/{*rest}"), ["id", "rest"]);
        assert_eq!(param_names("/users"), Vec::<String>::new());
    }
}
