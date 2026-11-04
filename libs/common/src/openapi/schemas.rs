//! Enterprise `OpenAPI` Common Schemas Registration.
//!
//! Registers common platform envelope and pagination schemas into the `OpenAPI` spec
//! components, satisfying Roadmap Step 8.1.

use utoipa::{
    Modify, PartialSchema, ToSchema,
    openapi::{OpenApi, RefOr, schema::Schema},
};

use crate::{
    i18n::ReloadLocalesResult,
    response::{
        CursorPaginationMeta, OffsetPaginationMeta, PaginatedResponseMeta, PaginationMeta,
        ResponseMeta,
    },
};

/// Modifier that registers common response envelopes and pagination schemas into `components.schemas`.
#[derive(Debug, Clone, Copy, Default)]
pub struct CommonSchemaAddon;

impl Modify for CommonSchemaAddon {
    fn modify(&self, openapi: &mut OpenApi) {
        register_common_schemas(openapi);
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

/// Helper function to register all omni-common schemas into an [`OpenApi`] instance.
pub fn register_common_schemas(openapi: &mut OpenApi) {
    let components = openapi.components.get_or_insert_with(Default::default);

    register_type_schema::<ResponseMeta>(&mut components.schemas);
    register_type_schema::<OffsetPaginationMeta>(&mut components.schemas);
    register_type_schema::<CursorPaginationMeta>(&mut components.schemas);
    register_type_schema::<PaginationMeta>(&mut components.schemas);
    register_type_schema::<PaginatedResponseMeta>(&mut components.schemas);
    register_type_schema::<ReloadLocalesResult>(&mut components.schemas);
}
