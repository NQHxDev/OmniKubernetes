//! Context bridging to omni-common `RequestContext` and OpenTelemetry Span attributes.

use omni_common::middleware::RequestContext;
use std::borrow::Cow;

use crate::extractor::AuthenticatedUser;

/// Đồng bộ định danh người dùng vào [`RequestContext`].
pub fn enrich_request_context(ctx: &mut RequestContext, user: &AuthenticatedUser) {
    ctx.user_id = Some(Cow::Owned(user.user_id.to_string()));
    ctx.tenant_id = user.tenant_id.map(|t| Cow::Owned(t.to_string()));
}

/// Đính kèm các trường định danh an toàn (không PII) vào OpenTelemetry active span.
pub fn enrich_tracing_span(user: &AuthenticatedUser) {
    let span = tracing::Span::current();
    span.record("user.id", tracing::field::display(user.user_id));
    if let Some(tenant_id) = user.tenant_id {
        span.record("tenant.id", tracing::field::display(tenant_id));
    }
    span.record("auth.token_id", tracing::field::display(user.token_id));
    span.record("auth.roles_count", user.roles.len() as u64);
}
