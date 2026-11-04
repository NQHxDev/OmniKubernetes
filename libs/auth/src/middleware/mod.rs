//! Authentication middlewares and Gateway Signed Context protocol verifier.

pub mod auth_layer;
pub mod signed_context;

pub use auth_layer::{AuthLayer, AuthService};
pub use signed_context::{
    GatewayContext, HEADER_OMNI_SIGNATURE, HEADER_OMNI_TIMESTAMP, HEADER_TENANT_ID, HEADER_USER_ID,
    HEADER_USER_ROLES, sign_gateway_context, verify_gateway_signature,
};
