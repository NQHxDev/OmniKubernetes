# Error localization strings (en-US)

# RFC 9457 HTTP status titles & details
err-bad-request-title = Bad Request
err-bad-request-detail = The request could not be processed due to invalid syntax or parameters.

err-unauthorized-title = Unauthorized
err-unauthorized-detail = Authentication credentials are required or invalid.

err-forbidden-title = Forbidden
err-forbidden-detail = You do not have permission to access this resource.

err-not-found-title = Resource Not Found
err-not-found-detail = The requested resource does not exist or has been removed.

err-method-not-allowed-title = Method Not Allowed
err-method-not-allowed-detail = The requested HTTP method is not supported for this endpoint.

err-conflict-title = Conflict
err-conflict-detail = A conflict occurred with the current state of the target resource.

err-validation-failed-title = Invalid Request Data
err-validation-failed-detail = {$count ->
    [one] 1 field failed business constraint validation.
   *[other] {$count} fields failed business constraint validation.
}

err-rate-limited-title = Too Many Requests
err-rate-limited-detail = Request rate limit exceeded. Please try again after {$retry_after} seconds.

err-internal-server-title = Internal Server Error
err-internal-server-detail = An unexpected internal error occurred. Please try again later.
err_internal_server = An unexpected internal error occurred. Please try again later.

err-service-unavailable-title = Service Unavailable
err-service-unavailable-detail = The service is temporarily unavailable. Please try again later.

# Field validation rules (RFC 9457 invalid_params)
val-required = Field '{$name}' is required.
val-invalid-email = Email address is not in a valid format.
val-string-min-length = {$name} must be at least {$min} characters (currently {$actual}).
val-string-max-length = {$name} must not exceed {$max} characters (currently {$actual}).
val-number-range = Field '{$name}' must be between {$min} and {$max}.
val-pattern-mismatch = Field '{$name}' does not match the required pattern.
val-invalid-uuid = Field '{$name}' must be a valid UUID.
val-invalid-format = Field '{$name}' format is invalid.

# Domain error codes
INVALID_EMAIL_FORMAT = Email address is not in a valid format.
STRING_MIN_LENGTH = {$name} must be at least {$min} characters (currently {$actual}).
NUMBER_MIN_VALUE = Item quantity in the order must be greater than 0.
INVALID_POSTAL_CODE = Postal code format is invalid.
ERR_VALIDATION_FAILED = Business constraint validation failed.

# OmniKubernetes cluster errors
err-cluster-unreachable = Cluster '{$cluster_name}' is unreachable at endpoint '{$endpoint}'.
err-namespace-not-found = Namespace '{$namespace}' does not exist in cluster '{$cluster_name}'.
err-pod-quota-exceeded = Pod creation rejected: quota exceeded in namespace '{$namespace}' (limit: {$limit}, current: {$current}).
err-deployment-failed = Deployment '{$deployment}' in namespace '{$namespace}' failed: {$reason}.
err-resource-conflict = Resource '{$resource}' of kind '{$kind}' already exists in namespace '{$namespace}'.

# Identity & Security Context (omni-auth)
err-auth-missing-credentials-title = Missing Authentication Credentials
err-auth-missing-credentials-detail = Authentication credentials are required in the Authorization header or request payload.
ERR_AUTH_MISSING_CREDENTIALS = Authentication credentials are required in the Authorization header or request payload.

err-auth-token-expired-title = Authentication Token Expired
err-auth-token-expired-detail = Your access token has expired. Please refresh your session using a valid refresh token.
ERR_AUTH_TOKEN_EXPIRED = Your access token has expired. Please refresh your session using a valid refresh token.

err-auth-token-revoked-title = Authentication Token Revoked
err-auth-token-revoked-detail = This token has been revoked due to user logout, password reset, or session invalidation.
ERR_AUTH_TOKEN_REVOKED = This token has been revoked due to user logout, password reset, or session invalidation.

err-auth-invalid-signature-title = Invalid Token Signature
err-auth-invalid-signature-detail = Cryptographic signature verification failed or token payload has been tampered with.
ERR_AUTH_INVALID_SIGNATURE = Cryptographic signature verification failed or token payload has been tampered with.

err-auth-malformed-token-title = Malformed Token Structure
err-auth-malformed-token-detail = The provided token does not conform to valid JSON Web Token format.
ERR_AUTH_MALFORMED_TOKEN = The provided token does not conform to valid JSON Web Token format.

err-auth-invalid-credentials-title = Invalid Credentials
err-auth-invalid-credentials-detail = The supplied username or password is incorrect.
ERR_AUTH_INVALID_CREDENTIALS = The supplied username or password is incorrect.

err-auth-insufficient-roles-title = Insufficient Role
err-auth-insufficient-roles-detail = You do not possess the required role to perform this action.
ERR_AUTH_INSUFFICIENT_ROLES = You do not possess the required role to perform this action.

err-auth-insufficient-perms-title = Insufficient Permission
err-auth-insufficient-perms-detail = You lack the specific permission required to access this resource.
ERR_AUTH_INSUFFICIENT_PERMS = You lack the specific permission required to access this resource.

err-auth-tenant-mismatch-title = Tenant Access Denied
err-auth-tenant-mismatch-detail = You are not authorized to access data in this tenant space.
ERR_AUTH_TENANT_MISMATCH = You are not authorized to access data in this tenant space.

err-auth-signature-expired-title = Internal Signature Expired
err-auth-signature-expired-detail = Internal gateway signature has expired or clock skew limit has been exceeded.
ERR_AUTH_SIGNATURE_EXPIRED = Internal gateway signature has expired or clock skew limit has been exceeded.

err-auth-token-theft-detected-title = Session Security Anomaly Detected
err-auth-token-theft-detected-detail = Refresh token reuse detected; entire token family invalidated for account security.
ERR_AUTH_TOKEN_THEFT_DETECTED = Refresh token reuse detected; entire token family invalidated for account security.
