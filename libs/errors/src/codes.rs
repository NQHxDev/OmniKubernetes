//! Standardized Machine-Readable Domain Error Codes & RFC 9457 Problem Type URIs.
//!
//! Provides compile-time static string constants used across `OmniKubernetes` microservices
//! so clients can handle errors deterministically without parsing localized human messages.

// ============================================================================
// RFC 9457 Problem Type URIs (https://errors.omnikubernetes.com/*)
// ============================================================================

/// Problem Type URI for HTTP 400 Bad Request.
pub const TYPE_BAD_REQUEST: &str = "https://errors.omnikubernetes.com/bad-request";

/// Problem Type URI for HTTP 401 Unauthorized.
pub const TYPE_UNAUTHORIZED: &str = "https://errors.omnikubernetes.com/unauthorized";

/// Problem Type URI for HTTP 403 Forbidden.
pub const TYPE_FORBIDDEN: &str = "https://errors.omnikubernetes.com/forbidden";

/// Problem Type URI for HTTP 404 Not Found.
pub const TYPE_NOT_FOUND: &str = "https://errors.omnikubernetes.com/not-found";

/// Problem Type URI for HTTP 405 Method Not Allowed.
pub const TYPE_METHOD_NOT_ALLOWED: &str = "https://errors.omnikubernetes.com/method-not-allowed";

/// Problem Type URI for HTTP 409 Conflict.
pub const TYPE_CONFLICT: &str = "https://errors.omnikubernetes.com/conflict";

/// Problem Type URI for HTTP 422 Unprocessable Entity (Validation Error).
pub const TYPE_VALIDATION_ERROR: &str = "https://errors.omnikubernetes.com/validation-error";

/// Problem Type URI for HTTP 429 Too Many Requests.
pub const TYPE_RATE_LIMITED: &str = "https://errors.omnikubernetes.com/rate-limited";

/// Problem Type URI for HTTP 500 Internal Server Error.
pub const TYPE_INTERNAL_SERVER_ERROR: &str =
    "https://errors.omnikubernetes.com/internal-server-error";

/// Problem Type URI for HTTP 503 Service Unavailable.
pub const TYPE_SERVICE_UNAVAILABLE: &str = "https://errors.omnikubernetes.com/service-unavailable";

// ============================================================================
// Top-Level Machine-Readable Domain Error Codes (`ApiErrorResponse.code`)
// ============================================================================

/// Domain error code for generic bad request syntax or parameters (HTTP 400).
pub const ERR_BAD_REQUEST: &str = "ERR_BAD_REQUEST";

/// Domain error code for missing or invalid authentication credentials (HTTP 401).
pub const ERR_UNAUTHORIZED: &str = "ERR_UNAUTHORIZED";

/// Domain error code for insufficient permissions (HTTP 403).
pub const ERR_FORBIDDEN: &str = "ERR_FORBIDDEN";

/// Domain error code for missing target resource (HTTP 404).
pub const ERR_NOT_FOUND: &str = "ERR_NOT_FOUND";

/// Domain error code for unsupported HTTP method (HTTP 405).
pub const ERR_METHOD_NOT_ALLOWED: &str = "ERR_METHOD_NOT_ALLOWED";

/// Domain error code for resource state conflict (HTTP 409).
pub const ERR_CONFLICT: &str = "ERR_CONFLICT";

/// Domain error code for business constraint or input validation failure (HTTP 422).
pub const ERR_VALIDATION_FAILED: &str = "ERR_VALIDATION_FAILED";

/// Domain error code for rate limiting / quota throttling (HTTP 429).
pub const ERR_RATE_LIMITED: &str = "ERR_RATE_LIMITED";

/// Domain error code for unexpected internal server failures (HTTP 500).
pub const ERR_INTERNAL_SERVER: &str = "ERR_INTERNAL_SERVER";

/// Domain error code for temporary service unavailability or maintenance (HTTP 503).
pub const ERR_SERVICE_UNAVAILABLE: &str = "ERR_SERVICE_UNAVAILABLE";

// ============================================================================
// Field-Level Validation Error Codes (`InvalidParamError.code`)
// ============================================================================

/// Validation code for missing mandatory field.
pub const VAL_REQUIRED: &str = "REQUIRED";

/// Validation code for invalid email format.
pub const VAL_INVALID_EMAIL_FORMAT: &str = "INVALID_EMAIL_FORMAT";

/// Validation code for string shorter than minimum allowed length.
pub const VAL_STRING_MIN_LENGTH: &str = "STRING_MIN_LENGTH";

/// Validation code for string exceeding maximum allowed length.
pub const VAL_STRING_MAX_LENGTH: &str = "STRING_MAX_LENGTH";

/// Validation code for numeric value below minimum threshold.
pub const VAL_NUMBER_MIN_VALUE: &str = "NUMBER_MIN_VALUE";

/// Validation code for numeric value above maximum threshold.
pub const VAL_NUMBER_MAX_VALUE: &str = "NUMBER_MAX_VALUE";

/// Validation code for numeric value outside `[min, max]` range.
pub const VAL_NUMBER_OUT_OF_RANGE: &str = "NUMBER_OUT_OF_RANGE";

/// Validation code for invalid postal code pattern.
pub const VAL_INVALID_POSTAL_CODE: &str = "INVALID_POSTAL_CODE";

/// Validation code for regular expression pattern mismatch.
pub const VAL_PATTERN_MISMATCH: &str = "PATTERN_MISMATCH";

/// Validation code for malformed UUID string.
pub const VAL_INVALID_UUID: &str = "INVALID_UUID";

/// Validation code for general malformed field format.
pub const VAL_INVALID_FORMAT: &str = "INVALID_FORMAT";

/// Validation code for unsupported enum variant.
pub const VAL_INVALID_ENUM_VALUE: &str = "INVALID_ENUM_VALUE";

// ============================================================================
// OmniKubernetes Infrastructure Error Codes
// ============================================================================

/// Cluster control-plane endpoint is unreachable.
pub const ERR_CLUSTER_UNREACHABLE: &str = "ERR_CLUSTER_UNREACHABLE";

/// Target Kubernetes namespace does not exist.
pub const ERR_NAMESPACE_NOT_FOUND: &str = "ERR_NAMESPACE_NOT_FOUND";

/// Pod creation rejected due to namespace quota limits.
pub const ERR_POD_QUOTA_EXCEEDED: &str = "ERR_POD_QUOTA_EXCEEDED";

/// Kubernetes deployment rollout or update failed.
pub const ERR_DEPLOYMENT_FAILED: &str = "ERR_DEPLOYMENT_FAILED";

/// Kubernetes resource already exists in the target namespace.
pub const ERR_RESOURCE_CONFLICT: &str = "ERR_RESOURCE_CONFLICT";
