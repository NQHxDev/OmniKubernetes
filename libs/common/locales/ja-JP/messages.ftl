# 成功・通知メッセージ (ja-JP)

# CRUD操作
msg-operation-success = 処理が正常に完了しました。
msg-resource-created = リソースが正常に作成されました。
msg-resource-updated = リソースが正常に更新されました。
msg-resource-deleted = リソースが正常に削除されました。

# プレースホルダー付き操作
msg-entity-created = {$entity} '{$id}' が正常に作成されました。
msg-entity-updated = {$entity} '{$id}' が正常に更新されました。
msg-entity-deleted = {$entity} '{$id}' が正常に削除されました。
msg-welcome-user = おかえりなさい、{$name}さん！ 最終ログイン: {$last_login}。
msg-batch-processed = {$count} 件の処理が {$duration_ms}ms で完了しました。

# 業務メッセージ
msg-user-fetch-success = ユーザー情報を取得しました。
msg-user-register-success = ユーザー '{$username}' の登録が完了しました。
msg-user-update-success = プロファイルを更新しました。
msg-user-delete-success = ユーザー '{$username}' を削除しました。

msg-order-list-success = 注文一覧を取得しました。
msg-order-fetch-success = 注文 '{$order_id}' の詳細を取得しました。
msg-order-create-success = 注文 '{$order_id}' を作成しました。
msg-order-cancel-success = 注文 '{$order_id}' をキャンセルしました。

msg-payment-process-success = {$amount} {$currency} の決済が完了しました。
msg-payment-refund-success = {$amount} {$currency} の返金処理を開始しました。

msg-audit-event-success = 監査ログを取得しました。
msg-notification-sent = '{$recipient}' へ通知を送信しました。

# OmniKubernetes クラスター操作
msg-cluster-connected = クラスター '{$cluster_name}' (Kubernetes v{$version}) に正常に接続しました。
msg-cluster-disconnected = クラスター '{$cluster_name}' から切断しました。
msg-deployment-scaled = ネームスペース '{$namespace}' のデプロイ '{$deployment}' のレプリカ数を {$from_replicas} から {$to_replicas} に変更しました。
msg-pod-restarted = ネームスペース '{$namespace}' のPod '{$pod_name}' を再起動しました。
msg-node-drained = ノード '{$node_name}' のドレインが完了しました ({$evicted_pods} 個のPodを退避)。
msg-namespace-cleanup = ネームスペース '{$namespace}' 内の {$count} 個のリソースをクリーンアップしました。
msg-configmap-reloaded = ConfigMap '{$name}' を再読み込みしました (バージョン {$version})。
