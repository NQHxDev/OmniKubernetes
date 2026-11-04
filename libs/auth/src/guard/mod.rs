//! Declarative authorization guards for route layers.

pub mod permission;
pub mod role;

pub use permission::{
    RequireAllPermissionsLayer, RequirePermissionLayer, require_all_permissions, require_permission,
};
pub use role::{RequireAnyRoleLayer, RequireRoleLayer, require_any_role, require_role};
