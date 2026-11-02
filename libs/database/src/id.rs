//! Strongly-typed, time-ordered Entity Identifier (`Id<T>`).
//!
//! Wraps RFC 9562 `UUIDv7` with phantom type checking to eliminate ID cross-assignment bugs
//! (e.g. passing a `CustomerId` where an `OrderId` was expected) while maintaining native
//! B-Tree index locality and direct serialization compatibility with PostgreSQL `UUID`.

use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Decode, Encode, Postgres, Type};
use uuid::Uuid;

/// Khóa chính kiểu tĩnh bọc `UUIDv7` sắp xếp theo thời gian.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id<T> {
    raw: Uuid,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Copy for Id<T> {}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Id<T> {
    /// Sinh một [`Id<T>`] mới sử dụng `UUIDv7` với timestamp hiện tại.
    #[must_use]
    pub fn new() -> Self {
        Self {
            raw: Uuid::now_v7(),
            _marker: PhantomData,
        }
    }

    /// Khởi tạo [`Id<T>`] từ một [`Uuid`] có sẵn.
    #[must_use]
    pub const fn from_uuid(raw: Uuid) -> Self {
        Self {
            raw,
            _marker: PhantomData,
        }
    }

    /// Tham chiếu đến [`Uuid`] thô bên trong.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.raw
    }

    /// Chuyển đổi thành [`Uuid`] thô.
    #[must_use]
    pub const fn into_uuid(self) -> Uuid {
        self.raw
    }

    /// Phân tích chuỗi UUID dạng chuẩn thành [`Id<T>`].
    pub fn parse_str(s: &str) -> Result<Self, uuid::Error> {
        let raw = Uuid::parse_str(s)?;
        Ok(Self::from_uuid(raw))
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.raw, f)
    }
}

impl<T> FromStr for Id<T> {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_str(s)
    }
}

impl<T> From<Uuid> for Id<T> {
    fn from(uuid: Uuid) -> Self {
        Self::from_uuid(uuid)
    }
}

impl<T> From<Id<T>> for Uuid {
    fn from(id: Id<T>) -> Self {
        id.into_uuid()
    }
}

impl<T> Serialize for Id<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.raw.serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for Id<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = Uuid::deserialize(deserializer)?;
        Ok(Self::from_uuid(raw))
    }
}

impl<T> Type<Postgres> for Id<T> {
    fn type_info() -> <Postgres as sqlx::Database>::TypeInfo {
        <Uuid as Type<Postgres>>::type_info()
    }
}

impl<'q, T: Send + Sync + 'static> Encode<'q, Postgres> for Id<T> {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as sqlx::Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        <Uuid as Encode<'q, Postgres>>::encode_by_ref(&self.raw, buf)
    }
}

impl<'r, T: 'static> Decode<'r, Postgres> for Id<T> {
    fn decode(value: <Postgres as sqlx::Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let raw = <Uuid as Decode<'r, Postgres>>::decode(value)?;
        Ok(Self::from_uuid(raw))
    }
}
