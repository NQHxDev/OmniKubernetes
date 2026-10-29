# エラー文字列 (ja-JP)

# RFC 9457 HTTPステータス
err-bad-request-title = 不正なリクエスト
err-bad-request-detail = 構文またはパラメータが無効なため、リクエストを処理できませんでした。

err-unauthorized-title = 未認証
err-unauthorized-detail = 認証情報が必要であるか、無効です。

err-forbidden-title = アクセス禁止
err-forbidden-detail = このリソースへのアクセス権限がありません。

err-not-found-title = リソースが見つかりません
err-not-found-detail = 要求されたリソースは存在しないか、削除されました。

err-method-not-allowed-title = 許可されていないメソッド
err-method-not-allowed-detail = 要求されたHTTPメソッドはこのエンドポイントでサポートされていません。

err-conflict-title = 競合が発生しました
err-conflict-detail = 対象リソースの現在の状態との競合が発生しました。

err-validation-failed-title = 入力データが無効です
err-validation-failed-detail = {$count} 件のフィールドが制約を満たしていません。

err-rate-limited-title = リクエスト数が上限を超えました
err-rate-limited-detail = リクエスト頻度が上限を超えました。{$retry_after} 秒後に再試行してください。

err-internal-server-title = 内部サーバーエラー
err-internal-server-detail = 予期しないエラーが発生しました。しばらくしてから再試行してください。
err_internal_server = 予期しないエラーが発生しました。しばらくしてから再試行してください。

err-service-unavailable-title = サービス利用不可
err-service-unavailable-detail = サービスは一時的に利用できません。しばらくしてから再試行してください。

# バリデーションルール (RFC 9457 invalid_params)
val-required = フィールド '{$name}' は必須です。
val-invalid-email = メールアドレスの形式が正しくありません。
val-string-min-length = {$name} は {$min} 文字以上である必要があります (現在: {$actual} 文字)。
val-string-max-length = {$name} は {$max} 文字以下である必要があります (現在: {$actual} 文字)。
val-number-range = フィールド '{$name}' は {$min} から {$max} の間である必要があります。
val-pattern-mismatch = フィールド '{$name}' は指定の形式に一致しません。
val-invalid-uuid = フィールド '{$name}' は有効なUUIDである必要があります。
val-invalid-format = フィールド '{$name}' の形式が無効です。

# ドメインエラーコード
INVALID_EMAIL_FORMAT = メールアドレスの形式が正しくありません。
STRING_MIN_LENGTH = {$name} は {$min} 文字以上である必要があります (現在: {$actual} 文字)。
NUMBER_MIN_VALUE = 注文内の商品数量は0より大きい必要があります。
INVALID_POSTAL_CODE = 郵便番号の形式が正しくありません。
ERR_VALIDATION_FAILED = バリデーション制約を満たしていません。

# OmniKubernetes クラスターエラー
err-cluster-unreachable = エンドポイント '{$endpoint}' のクラスター '{$cluster_name}' に接続できません。
err-namespace-not-found = クラスター '{$cluster_name}' にネームスペース '{$namespace}' は存在しません。
err-pod-quota-exceeded = Podの作成が拒否されました: ネームスペース '{$namespace}' のクォータ超過 (上限: {$limit}、現在: {$current})。
err-deployment-failed = ネームスペース '{$namespace}' のデプロイ '{$deployment}' に失敗しました: {$reason}。
err-resource-conflict = ネームスペース '{$namespace}' に種類 '{$kind}' のリソース '{$resource}' が既に存在します。
