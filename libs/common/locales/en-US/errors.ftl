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
