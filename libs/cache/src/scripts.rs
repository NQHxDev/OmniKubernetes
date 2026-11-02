//! Atomic Redis Lua Scripts for Concurrency Coordination & Distributed Lease (`scripts.rs`).
//!
//! Prepares and manages cluster-compatible Lua scripts with automatic `EVALSHA` caching
//! and hash tag alignment.

use redis::Script;

// TODO: Nâng lên UUIDv7 được không?

/// Script chiếm khóa phân tán nguyên tử kèm cấp phát Fencing Token:
/// - `KEYS[1]`: Lock key (`...:lock:{ns}:r:<id>`)
/// - `KEYS[2]`: Fencing counter key (`...:lock:{ns}:fence`)
/// - `ARGV[1]`: Owner token `UUIDv4`
/// - `ARGV[2]`: TTL in milliseconds
///
/// Trả về mảng `[status, fence_token]`: `[1, fence]` nếu thành công, `[0, 0]` nếu thất bại.
pub const SCRIPT_ACQUIRE_WITH_FENCE: &str = r"
local acquired = redis.call('SET', KEYS[1], ARGV[1], 'NX', 'PX', ARGV[2])
if acquired then
    local fence = redis.call('INCR', KEYS[2])
    return {1, fence}
else
    return {0, 0}
end
";

/// Script giải phóng khóa an toàn (Compare-and-Delete):
/// - `KEYS[1]`: Lock key
/// - `ARGV[1]`: Owner token
///
/// Trả về `1` nếu đã xóa, `0` nếu khóa không thuộc sở hữu của caller hoặc đã hết hạn.
pub const SCRIPT_RELEASE: &str = r"
if redis.call('GET', KEYS[1]) == ARGV[1] then
    return redis.call('DEL', KEYS[1])
else
    return 0
end
";

/// Script gia hạn thời hạn khóa phân tán (Heartbeat Extend):
/// - `KEYS[1]`: Lock key
/// - `ARGV[1]`: Owner token
/// - `ARGV[2]`: New TTL in milliseconds
///
/// Trả về `1` nếu gia hạn thành công, `0` nếu khóa đã mất quyền sở hữu.
pub const SCRIPT_EXTEND: &str = r"
if redis.call('GET', KEYS[1]) == ARGV[1] then
    return redis.call('PEXPIRE', KEYS[1], ARGV[2])
else
    return 0
end
";

/// Script ghi dữ liệu kèm kiểm tra Distributed Lease (chống Stale Set):
/// - `KEYS[1]`: Data cache key (`omni:...:{id}`)
/// - `KEYS[2]`: Lease key (`omni:...:{id}:lease`)
/// - `ARGV[1]`: Lease token
/// - `ARGV[2]`: Envelope payload bytes
/// - `ARGV[3]`: TTL in milliseconds
///
/// Trả về `1` nếu ghi thành công và xóa lease, `0` nếu lease đã hết hạn hoặc bị chiếm.
pub const SCRIPT_WRITE_WITH_LEASE: &str = r"
if redis.call('GET', KEYS[2]) == ARGV[1] then
    redis.call('SET', KEYS[1], ARGV[2], 'PX', ARGV[3])
    redis.call('DEL', KEYS[2])
    return 1
else
    return 0
end
";

/// Bộ khởi tạo các [`Script`] sẵn sàng thực thi.
#[derive(Debug, Clone)]
pub struct LuaScripts {
    pub acquire_fence: Script,
    pub release: Script,
    pub extend: Script,
    pub write_lease: Script,
}

impl Default for LuaScripts {
    fn default() -> Self {
        Self::new()
    }
}

impl LuaScripts {
    /// Khởi tạo và biên dịch các scripts.
    #[must_use]
    pub fn new() -> Self {
        Self {
            acquire_fence: Script::new(SCRIPT_ACQUIRE_WITH_FENCE),
            release: Script::new(SCRIPT_RELEASE),
            extend: Script::new(SCRIPT_EXTEND),
            write_lease: Script::new(SCRIPT_WRITE_WITH_LEASE),
        }
    }
}
