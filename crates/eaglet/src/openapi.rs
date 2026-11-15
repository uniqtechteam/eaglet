//! API docs: what the controller macro records about each route, the OpenAPI 3.1 document built from
//! it (the `openapi` feature) and the Swagger UI page (the `swagger` feature).
//!
//! DTOs that derive `utoipa::ToSchema` appear with their full schema; any other type still works
//! and shows as "any value".

use std::marker::PhantomData;

/// The schema of one type, or "any value" when the type has no `ToSchema`.
#[doc(hidden)]
#[derive(Clone, Default)]
pub struct SchemaDoc {
    #[cfg(feature = "openapi")]
    schema: Option<schema::RefOr<schema::Schema>>,
    #[cfg(feature = "openapi")]
    components: Vec<(String, schema::RefOr<schema::Schema>)>,
}

impl std::fmt::Debug for SchemaDoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemaDoc").finish_non_exhaustive()
    }
}

/// What a route returns when it succeeds.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct ResponseDoc {
    /// `"200"`, or `"2XX"` when the handler picks the status itself.
    pub status: &'static str,
    /// `application/json`, `text/plain`, or none for an empty body.
    pub content_type: Option<&'static str>,
    /// The body's schema, for JSON.
    pub schema: Option<SchemaDoc>,
}

/// One route, as the controller macro saw it.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct RouteDoc {
    /// `get`, `post`, ...
    pub method: &'static str,
    /// The Axum path, for example `/users/{id}`.
    pub path: &'static str,
    /// `UsersController_find`.
    pub operation_id: &'static str,
    /// The controller name without `Controller`.
    pub tag: &'static str,
    /// The method's doc comment.
    pub docs: &'static str,
    /// Path parameters in order.
    pub path_params: Vec<(&'static str, SchemaDoc)>,
    /// The `Query<T>` type.
    pub query: Option<SchemaDoc>,
    /// The `Json<T>` body type.
    pub body: Option<SchemaDoc>,
    /// The success response.
    pub response: ResponseDoc,
}

/// Autoref specialisation: `(&Probe::<T>::new()).schema_doc()` uses `ToSchema` when `T` has it and
/// falls back to "any value" when it does not, so DTOs never have to derive it.
#[doc(hidden)]
pub struct Probe<T: ?Sized>(PhantomData<fn(&T)>);

impl<T: ?Sized> Probe<T> {
    #[must_use]
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: ?Sized> Default for Probe<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Chosen when the type has a schema.
#[doc(hidden)]
pub trait ViaToSchema {
    fn schema_doc(&self) -> SchemaDoc;
}

/// Chosen otherwise.
#[doc(hidden)]
pub trait ViaAny {
    fn schema_doc(&self) -> SchemaDoc;
}

impl<T: ?Sized> ViaAny for &Probe<T> {
    fn schema_doc(&self) -> SchemaDoc {
        SchemaDoc::any()
    }
}

impl SchemaDoc {
    /// Any value.
    #[must_use]
    pub fn any() -> Self {
        Self::default()
    }

    /// A string, for path parameters whose type is not known.
    #[must_use]
    pub fn string() -> Self {
        #[cfg(feature = "openapi")]
        {
            let schema = schema::Object::with_type(schema::Type::String);
            Self {
                schema: Some(schema::RefOr::T(schema::Schema::Object(schema))),
                components: Vec::new(),
            }
        }
        #[cfg(not(feature = "openapi"))]
        Self::default()
    }

    /// The schema of field `name` inside `of` (for `Path<Params>` structs).
    #[must_use]
    pub fn property(of: &Self, name: &str) -> Self {
        #[cfg(feature = "openapi")]
        {
            of.object()
                .and_then(|o| o.properties.get(name).cloned())
                .map_or_else(Self::string, |p| Self {
                    schema: Some(p),
                    components: of.components.clone(),
                })
        }
        #[cfg(not(feature = "openapi"))]
        {
            let _ = (of, name);
            Self::string()
        }
    }
}

#[cfg(feature = "openapi")]
pub(crate) use doc::document;

#[cfg(feature = "openapi")]
mod schema {
    pub use utoipa::openapi::schema::{Object, Schema, SchemaType, Type};
    pub use utoipa::openapi::{Ref, RefOr};
}

#[cfg(feature = "openapi")]
impl<T: utoipa::ToSchema + ?Sized> ViaToSchema for Probe<T> {
    fn schema_doc(&self) -> SchemaDoc {
        let mut components = Vec::new();
        T::schemas(&mut components);
        let schema = T::schema();
        let named = match &schema {
            schema::RefOr::T(schema::Schema::Object(o)) => {
                o.schema_type == schema::SchemaType::Type(schema::Type::Object)
            }
            schema::RefOr::T(
                schema::Schema::OneOf(_) | schema::Schema::AllOf(_) | schema::Schema::AnyOf(_),
            ) => true,
            _ => false,
        };
        if named {
            let name = T::name().into_owned();
            components.push((name.clone(), schema));
            SchemaDoc {
                schema: Some(schema::RefOr::Ref(schema::Ref::from_schema_name(name))),
                components,
            }
        } else {
            SchemaDoc {
                schema: Some(schema),
                components,
            }
        }
    }
}

#[cfg(feature = "openapi")]
impl SchemaDoc {
    /// The object behind this schema, following a `$ref` into the components it carries.
    fn object(&self) -> Option<&schema::Object> {
        let target = match self.schema.as_ref()? {
            schema::RefOr::T(s) => s,
            schema::RefOr::Ref(r) => {
                let name = r.ref_location.rsplit('/').next()?;
                match &self.components.iter().find(|(n, _)| n == name)?.1 {
                    schema::RefOr::T(s) => s,
                    schema::RefOr::Ref(_) => return None,
                }
            }
        };
        match target {
            schema::Schema::Object(o) => Some(o),
            _ => None,
        }
    }
}

#[cfg(feature = "openapi")]
mod doc {
    use std::collections::BTreeMap;

    use utoipa::openapi::path::{
        HttpMethod, OperationBuilder, ParameterBuilder, ParameterIn, Paths,
    };
    use utoipa::openapi::request_body::RequestBodyBuilder;
    use utoipa::openapi::schema::{Object, ObjectBuilder, Schema, Type};
    use utoipa::openapi::{
        ComponentsBuilder, ContentBuilder, InfoBuilder, OpenApi, OpenApiBuilder, Ref, RefOr,
        Required, ResponseBuilder, ResponsesBuilder,
    };

    use super::{RouteDoc, SchemaDoc};

    const ERROR_SCHEMA: &str = "HttpError";

    fn method(name: &str) -> HttpMethod {
        match name {
            "post" => HttpMethod::Post,
            "put" => HttpMethod::Put,
            "patch" => HttpMethod::Patch,
            "delete" => HttpMethod::Delete,
            _ => HttpMethod::Get,
        }
    }

    /// Axum's `{*rest}` is `{rest}` in OpenAPI.
    fn openapi_path(path: &str) -> String {
        path.replace("{*", "{")
    }

    /// The envelope every error uses.
    fn error_schema() -> RefOr<Schema> {
        let text = || Object::with_type(Type::String);
        ObjectBuilder::new()
            .property("code", text())
            .property("message", text())
            .property("details", Object::new())
            .property(
                "traceId",
                ObjectBuilder::new().schema_type(utoipa::openapi::schema::SchemaType::from_iter([
                    Type::String,
                    Type::Null,
                ])),
            )
            .required("code")
            .required("message")
            .into()
    }

    fn keep(doc: &SchemaDoc, components: &mut BTreeMap<String, RefOr<Schema>>) -> RefOr<Schema> {
        for (name, schema) in &doc.components {
            components
                .entry(name.clone())
                .or_insert_with(|| schema.clone());
        }
        doc.schema
            .clone()
            .unwrap_or_else(|| RefOr::T(Schema::Object(Object::new())))
    }

    fn operation(
        route: &RouteDoc,
        components: &mut BTreeMap<String, RefOr<Schema>>,
    ) -> utoipa::openapi::path::Operation {
        let mut lines = route.docs.lines().map(str::trim);
        let summary = lines.next().filter(|l| !l.is_empty());
        let rest = lines.collect::<Vec<_>>().join("\n");
        let mut op = OperationBuilder::new()
            .operation_id(Some(route.operation_id))
            .tag(route.tag)
            .summary(summary)
            .description((!rest.trim().is_empty()).then(|| rest.trim().to_owned()));
        for (name, schema) in &route.path_params {
            op = op.parameter(
                ParameterBuilder::new()
                    .name(*name)
                    .parameter_in(ParameterIn::Path)
                    .required(Required::True)
                    .schema(Some(keep(schema, components))),
            );
        }
        if let Some(query) = &route.query {
            keep(query, components);
            if let Some(object) = query.object() {
                for (name, schema) in &object.properties {
                    let required = if object.required.contains(name) {
                        Required::True
                    } else {
                        Required::False
                    };
                    op = op.parameter(
                        ParameterBuilder::new()
                            .name(name)
                            .parameter_in(ParameterIn::Query)
                            .required(required)
                            .schema(Some(schema.clone())),
                    );
                }
            }
        }
        if let Some(body) = &route.body {
            let content = ContentBuilder::new()
                .schema(Some(keep(body, components)))
                .build();
            op = op.request_body(Some(
                RequestBodyBuilder::new()
                    .content("application/json", content)
                    .required(Some(Required::True))
                    .build(),
            ));
        }
        let mut ok = ResponseBuilder::new().description("Success");
        if let Some(content_type) = route.response.content_type {
            let schema = route.response.schema.as_ref().map_or_else(
                || RefOr::T(Schema::Object(Object::with_type(Type::String))),
                |s| keep(s, components),
            );
            ok = ok.content(
                content_type,
                ContentBuilder::new().schema(Some(schema)).build(),
            );
        }
        let error = ResponseBuilder::new()
            .description("An error, in the envelope every eaglet error uses")
            .content(
                "application/json",
                ContentBuilder::new()
                    .schema(Some(RefOr::Ref(Ref::from_schema_name(ERROR_SCHEMA))))
                    .build(),
            );
        op.responses(
            ResponsesBuilder::new()
                .response(route.response.status, ok)
                .response("default", error),
        )
        .build()
    }

    /// The OpenAPI 3.1 document for `routes`.
    pub(crate) fn document(title: &str, version: &str, routes: &[RouteDoc]) -> OpenApi {
        let mut components = BTreeMap::new();
        components.insert(ERROR_SCHEMA.to_owned(), error_schema());
        let mut paths = Paths::new();
        for route in routes {
            let op = operation(route, &mut components);
            paths.add_path_operation(openapi_path(route.path), vec![method(route.method)], op);
        }
        OpenApiBuilder::new()
            .info(InfoBuilder::new().title(title).version(version))
            .paths(paths)
            .components(Some(
                ComponentsBuilder::new()
                    .schemas_from_iter(components)
                    .build(),
            ))
            .build()
    }
}

/// The Swagger UI page. The assets come from a pinned `swagger-ui-dist` with integrity hashes, so
/// nothing is downloaded at build time and the browser refuses a changed file.
#[cfg(feature = "swagger")]
pub(crate) fn swagger_html(title: &str, json_url: &str) -> String {
    const VERSION: &str = "5.34.1";
    const JS_SRI: &str = "sha384-XFYN47VzPiDY6qPtxYKg0nqUeW/upo20xf1j2SUE7gT/i35FJB4cuS/71Iegzk8Z";
    const CSS_SRI: &str = "sha384-4c3vsE5f+trhFyHrFrLM1fFGUn531NIaC7dcqEWIJdoRQJ5CkJbSuSCwXLpaS1KF";
    let escape = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let title = escape(title);
    let url = serde_json::to_string(json_url).unwrap_or_else(|_| "\"\"".to_owned());
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@{VERSION}/swagger-ui.css" integrity="{CSS_SRI}" crossorigin="anonymous">
</head>
<body>
<div id="swagger-ui"></div>
<script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@{VERSION}/swagger-ui-bundle.js" integrity="{JS_SRI}" crossorigin="anonymous"></script>
<script>window.ui = SwaggerUIBundle({{ url: {url}, dom_id: "#swagger-ui" }});</script>
</body>
</html>
"##
    )
}
