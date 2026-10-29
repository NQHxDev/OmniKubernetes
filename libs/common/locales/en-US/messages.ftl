# Success & notification messages (en-US)

# CRUD operations
msg-operation-success = Operation completed successfully.
msg-resource-created = Resource created successfully.
msg-resource-updated = Resource updated successfully.
msg-resource-deleted = Resource deleted successfully.

# Operations with placeholders
msg-entity-created = Successfully created {$entity} '{$id}'.
msg-entity-updated = Successfully updated {$entity} '{$id}'.
msg-entity-deleted = Successfully deleted {$entity} '{$id}'.
msg-welcome-user = Welcome back, {$name}! Last login: {$last_login}.
msg-batch-processed = {$count ->
    [one] Successfully processed 1 item in {$duration_ms}ms.
   *[other] Successfully processed {$count} items in {$duration_ms}ms.
}

# Domain messages
msg-user-fetch-success = User information retrieved successfully.
msg-user-register-success = User account registered successfully for '{$username}'.
msg-user-update-success = User profile updated successfully.
msg-user-delete-success = User account '{$username}' deleted successfully.

msg-order-list-success = Order list retrieved successfully.
msg-order-fetch-success = Order details retrieved successfully for order '{$order_id}'.
msg-order-create-success = Order '{$order_id}' created successfully.
msg-order-cancel-success = Order '{$order_id}' cancelled successfully.

msg-payment-process-success = Payment of {$amount} {$currency} processed successfully.
msg-payment-refund-success = Payment refund of {$amount} {$currency} initiated successfully.

msg-audit-event-success = Audit event trail retrieved successfully.
msg-notification-sent = Notification sent successfully to '{$recipient}'.

# OmniKubernetes cluster operations
msg-cluster-connected = Successfully connected to cluster '{$cluster_name}' (Kubernetes v{$version}).
msg-cluster-disconnected = Disconnected from cluster '{$cluster_name}'.
msg-deployment-scaled = Scaled deployment '{$deployment}' in namespace '{$namespace}' from {$from_replicas} to {$to_replicas} replicas.
msg-pod-restarted = Pod '{$pod_name}' in namespace '{$namespace}' restarted successfully.
msg-node-drained = Successfully drained node '{$node_name}' ({$evicted_pods} pods evicted).
msg-namespace-cleanup = Cleaned up {$count} resources in namespace '{$namespace}'.
msg-configmap-reloaded = ConfigMap '{$name}' reloaded successfully (version {$version}).
