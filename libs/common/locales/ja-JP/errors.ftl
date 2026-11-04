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

# 認証・認可コンテキスト (omni-auth)
err-auth-missing-credentials-title = 認証情報が不足しています
err-auth-missing-credentials-detail = Authorizationヘッダーまたはリクエストに有効な認証情報が必要です。
ERR_AUTH_MISSING_CREDENTIALS = Authorizationヘッダーまたはリクエストに有効な認証情報が必要です。

err-auth-token-expired-title = 認証トークンの有効期限切れ
err-auth-token-expired-detail = アクセストークンの有効期限が切れています。リフレッシュトークンで再認証してください。
ERR_AUTH_TOKEN_EXPIRED = アクセストークンの有効期限が切れています。リフレッシュトークンで再認証してください。

err-auth-token-revoked-title = 認証トークンは失効しています
err-auth-token-revoked-detail = ログアウトやパスワード変更により、このトークンは失効しました。
ERR_AUTH_TOKEN_REVOKED = ログアウトやパスワード変更により、このトークンは失効しました。

err-auth-invalid-signature-title = 無効なトークン署名
err-auth-invalid-signature-detail = トークンのデジタル署名検証に失敗したか、データが改ざんされています。
ERR_AUTH_INVALID_SIGNATURE = トークンのデジタル署名検証に失敗したか、データが改ざんされています。

err-auth-malformed-token-title = 不正な形式のトークン
err-auth-malformed-token-detail = 提供されたトークンはJWTの標準仕様に準拠していません。
ERR_AUTH_MALFORMED_TOKEN = 提供されたトークンはJWTの標準仕様に準拠していません。

err-auth-invalid-credentials-title = 認証情報が正しくありません
err-auth-invalid-credentials-detail = ユーザー名またはパスワードが無効です。
ERR_AUTH_INVALID_CREDENTIALS = ユーザー名またはパスワードが無効です。

err-auth-insufficient-roles-title = 必要なロールが不足しています
err-auth-insufficient-roles-detail = この操作を実行するために必要なロールを保持していません。
ERR_AUTH_INSUFFICIENT_ROLES = この操作を実行するために必要なロールを保持していません。

err-auth-insufficient-perms-title = 必要な権限が不足しています
err-auth-insufficient-perms-detail = このリソースにアクセスするために必要な権限がありません。
ERR_AUTH_INSUFFICIENT_PERMS = このリソースにアクセスするために必要な権限がありません。

err-auth-tenant-mismatch-title = テナントアクセスが拒否されました
err-auth-tenant-mismatch-detail = このテナントのデータ領域へのアクセス権限がありません。
ERR_AUTH_TENANT_MISMATCH = このテナントのデータ領域へのアクセス権限がありません。

err-auth-signature-expired-title = 内部署名の有効期限切れ
err-auth-signature-expired-detail = ゲートウェイ署名の有効期限が切れているか、クロックスキューの許容範囲を超えています。
ERR_AUTH_SIGNATURE_EXPIRED = ゲートウェイ署名の有効期限が切れているか、クロックスキューの許容範囲を超えています。

err-auth-token-theft-detected-title = セッションのセキュリティ警告
err-auth-token-theft-detected-detail = 失効済みリフレッシュトークンの再利用を検知しました。安全のため関連セッションを無効化しました。
ERR_AUTH_TOKEN_THEFT_DETECTED = 失効済みリフレッシュトークンの再利用を検知しました。安全のため関連セッションを無効化しました。
