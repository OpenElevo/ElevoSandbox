//! Permission domain model

use serde::{Deserialize, Serialize};
use std::fmt;

/// Permission level for share access
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionLevel {
    Read,
    Write,
    Execute,
    Admin,
}

impl PermissionLevel {
    /// Check if this permission level includes the required level
    pub fn includes(&self, required: &PermissionLevel) -> bool {
        self.as_int() >= required.as_int()
    }

    fn as_int(&self) -> i32 {
        match self {
            PermissionLevel::Read => 1,
            PermissionLevel::Write => 2,
            PermissionLevel::Execute => 3,
            PermissionLevel::Admin => 4,
        }
    }

    pub fn from_str_value(s: &str) -> Option<Self> {
        match s {
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            "execute" => Some(Self::Execute),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionLevel::Read => "read",
            PermissionLevel::Write => "write",
            PermissionLevel::Execute => "execute",
            PermissionLevel::Admin => "admin",
        }
    }
}

impl fmt::Display for PermissionLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_includes_same_level() {
        assert!(PermissionLevel::Read.includes(&PermissionLevel::Read));
        assert!(PermissionLevel::Write.includes(&PermissionLevel::Write));
        assert!(PermissionLevel::Execute.includes(&PermissionLevel::Execute));
        assert!(PermissionLevel::Admin.includes(&PermissionLevel::Admin));
    }

    #[test]
    fn test_permission_includes_lower_level() {
        assert!(PermissionLevel::Admin.includes(&PermissionLevel::Read));
        assert!(PermissionLevel::Admin.includes(&PermissionLevel::Write));
        assert!(PermissionLevel::Admin.includes(&PermissionLevel::Execute));
        assert!(PermissionLevel::Execute.includes(&PermissionLevel::Read));
        assert!(PermissionLevel::Execute.includes(&PermissionLevel::Write));
        assert!(PermissionLevel::Write.includes(&PermissionLevel::Read));
    }

    #[test]
    fn test_permission_excludes_higher_level() {
        assert!(!PermissionLevel::Read.includes(&PermissionLevel::Write));
        assert!(!PermissionLevel::Read.includes(&PermissionLevel::Execute));
        assert!(!PermissionLevel::Read.includes(&PermissionLevel::Admin));
        assert!(!PermissionLevel::Write.includes(&PermissionLevel::Execute));
        assert!(!PermissionLevel::Write.includes(&PermissionLevel::Admin));
        assert!(!PermissionLevel::Execute.includes(&PermissionLevel::Admin));
    }

    #[test]
    fn test_from_str_value() {
        assert_eq!(PermissionLevel::from_str_value("read"), Some(PermissionLevel::Read));
        assert_eq!(PermissionLevel::from_str_value("write"), Some(PermissionLevel::Write));
        assert_eq!(PermissionLevel::from_str_value("execute"), Some(PermissionLevel::Execute));
        assert_eq!(PermissionLevel::from_str_value("admin"), Some(PermissionLevel::Admin));
        assert_eq!(PermissionLevel::from_str_value("unknown"), None);
        assert_eq!(PermissionLevel::from_str_value(""), None);
    }

    #[test]
    fn test_as_str_roundtrip() {
        for level in [PermissionLevel::Read, PermissionLevel::Write, PermissionLevel::Execute, PermissionLevel::Admin] {
            assert_eq!(PermissionLevel::from_str_value(level.as_str()), Some(level));
        }
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", PermissionLevel::Read), "read");
        assert_eq!(format!("{}", PermissionLevel::Admin), "admin");
    }

    #[test]
    fn test_serde_roundtrip() {
        let json = serde_json::to_string(&PermissionLevel::Write).unwrap();
        assert_eq!(json, "\"write\"");
        let parsed: PermissionLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, PermissionLevel::Write);
    }
}

/// Share permission record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharePermission {
    pub tenant_id: String,
    pub share_id: String,
    pub permission: PermissionLevel,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
