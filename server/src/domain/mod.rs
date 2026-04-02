//! Domain models

use uuid::Uuid;

/// Extension trait to format `Uuid` as simple string (no hyphens).
pub trait UuidSimple {
    /// Returns the UUID as a simple string without hyphens.
    fn simple_string(&self) -> String;
}

impl UuidSimple for Uuid {
    fn simple_string(&self) -> String {
        self.simple().to_string()
    }
}

pub mod audit;
pub mod auth;
pub mod oidc;
pub mod permission;
pub mod sandbox;
pub mod share;
pub mod tenant;
pub mod types;
pub mod workspace;

/// Serde helper: serialize/deserialize `Uuid` as simple string (no hyphens).
pub mod simple_uuid {
    use serde::{Deserialize, Deserializer, Serializer};
    use uuid::Uuid;

    pub fn serialize<S: Serializer>(uuid: &Uuid, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&uuid.simple().to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Uuid, D::Error> {
        let s = String::deserialize(d)?;
        s.parse::<Uuid>().map_err(serde::de::Error::custom)
    }

    pub fn serialize_option<S: Serializer>(
        uuid: &Option<Uuid>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        match uuid {
            Some(u) => s.serialize_some(&u.simple().to_string()),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize_option<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<Uuid>, D::Error> {
        let opt: Option<String> = Option::deserialize(d)?;
        match opt {
            Some(s) => s
                .parse::<Uuid>()
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}
