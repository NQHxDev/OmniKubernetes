//! `OpenAPI` Schema Registration for RFC 9457 Problem Details.
//!
//! Provides automated registration of [`ApiErrorResponse`] and [`InvalidParamError`]
//! schemas into `OpenAPI` components, fulfilling Roadmap Step 8.1.

use utoipa::{
    Modify, PartialSchema, ToSchema,
    openapi::{OpenApi, RefOr, schema::Schema},
};

use crate::problem::{ApiErrorResponse, InvalidParamError};

/// `OpenAPI` Modifier for registering RFC 9457 Problem Details schemas into `components.schemas`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ErrorSchemaAddon;

impl Modify for ErrorSchemaAddon {
    fn modify(&self, openapi: &mut OpenApi) {
        register_error_schemas(openapi);
    }
}

fn register_type_schema<T: ToSchema + PartialSchema>(
    schemas: &mut std::collections::BTreeMap<String, RefOr<Schema>>,
) {
    let name = <T as ToSchema>::name();
    let schema = <T as PartialSchema>::schema();
    schemas.insert(name.to_string(), schema);

    let mut sub_schemas = Vec::new();
    <T as ToSchema>::schemas(&mut sub_schemas);
    for (sub_name, sub_schema) in sub_schemas {
        schemas.insert(sub_name, sub_schema);
    }
}

/// Registers [`InvalidParamError`] and [`ApiErrorResponse`] into the provided [`OpenApi`] components.
pub fn register_error_schemas(openapi: &mut OpenApi) {
    let components = openapi.components.get_or_insert_with(Default::default);

    register_type_schema::<InvalidParamError>(&mut components.schemas);
    register_type_schema::<ApiErrorResponse>(&mut components.schemas);
}
