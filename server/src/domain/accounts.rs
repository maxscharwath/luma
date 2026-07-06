//! Account types: users, capability permissions, the public profile-picker
//! shape and registration invites.
//!
//! The JSON shape here is a public contract web/TV clients depend on it, so
//! field names and casing must not drift.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A user account. `password_hash` lives only in the DB layer and is never part
/// of this (serialized) shape, so a `User` is always safe to send to clients.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct User {
    pub id: String,
    pub email: String,
    pub username: String,
    #[serde(rename = "avatarUrl", skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// Preferred UI locale (`"fr"` | `"en"`), synced across this account's
    /// devices. `None` → clients fall back to the device/browser locale. See the
    /// shared i18n catalogs (`packages/core/src/locales`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Granted permissions (capability-based, no roles). The first registered
    /// account (owner) gets every permission; the rest default to `[Playback]`.
    /// Clients unlock pages/actions from this set (admin panel, future stats…).
    pub permissions: Vec<Permission>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    /// Whether this account has a numeric profile-lock PIN set on the server
    /// (`pin_hash IS NOT NULL`). Lets a client show its own PIN state (e.g. the
    /// TV profile menu's "Change PIN" vs "Set PIN"); the PIN itself never leaves
    /// the server. See `/api/auth/pin/*`.
    #[serde(rename = "hasPin")]
    pub has_pin: bool,
}

impl User {
    /// Whether this user holds a given permission. Gates the invite/admin
    /// endpoints via [`crate::api::users`]'s `require`.
    pub fn can(&self, perm: Permission) -> bool {
        self.permissions.contains(&perm)
    }
}

/// A granular capability. Stored on each user as a JSON array of the string keys
/// below. Extend this enum (and the TS mirror in `@luma/core`) to add more
/// e.g. a `stats.view` for the upcoming stats pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Permission {
    /// Manage user accounts (the admin panel).
    #[serde(rename = "users.manage")]
    UsersManage,
    /// Manage libraries, scans and metadata.
    #[serde(rename = "library.manage")]
    LibraryManage,
    /// Manage server settings.
    #[serde(rename = "settings.manage")]
    SettingsManage,
    /// Watch the catalogue and save playback progress (default for everyone).
    #[serde(rename = "playback")]
    Playback,
    /// Submit media requests (discover on TMDB + ask for a title).
    #[serde(rename = "requests.create")]
    RequestsCreate,
    /// Review the request queue: approve/deny anyone's requests, run
    /// interactive searches and manage downloads.
    #[serde(rename = "requests.manage")]
    RequestsManage,
    /// This user's requests skip the approval queue (Overseerr's auto-approve).
    #[serde(rename = "requests.auto")]
    RequestsAuto,
}

impl Permission {
    /// Parse a stored key; `None` for unknown keys (tolerant forward-compat).
    pub fn parse(s: &str) -> Option<Permission> {
        match s {
            "users.manage" => Some(Permission::UsersManage),
            "library.manage" => Some(Permission::LibraryManage),
            "settings.manage" => Some(Permission::SettingsManage),
            "playback" => Some(Permission::Playback),
            "requests.create" => Some(Permission::RequestsCreate),
            "requests.manage" => Some(Permission::RequestsManage),
            "requests.auto" => Some(Permission::RequestsAuto),
            _ => None,
        }
    }

    /// Every permission granted to the owner account.
    pub fn all() -> Vec<Permission> {
        vec![
            Permission::UsersManage,
            Permission::LibraryManage,
            Permission::SettingsManage,
            Permission::Playback,
            Permission::RequestsCreate,
            Permission::RequestsManage,
            Permission::RequestsAuto,
        ]
    }
}

/// Derive a display role label from a capability set. The backend is
/// capability-based; this is purely for the admin UI's "Rôle" badge.
pub fn role_label(perms: &[Permission]) -> &'static str {
    if perms.contains(&Permission::UsersManage) && perms.contains(&Permission::SettingsManage) {
        "Propriétaire"
    } else if perms.contains(&Permission::Playback) {
        "Membre"
    } else {
        "Restreint"
    }
}

/// The publicly-listable subset of a user, surfaced by `GET /api/users` to
/// populate the "Qui regarde ?" profile picker (no email).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PublicUser {
    pub id: String,
    pub username: String,
    #[serde(rename = "avatarUrl", skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// Whether this account has a profile-lock PIN (`pin_hash IS NOT NULL`), so
    /// the "Qui regarde ?" picker can render a lock and route to the PIN screen
    /// before switching in. Defaults to `false` for accounts without one.
    #[serde(rename = "hasPin")]
    pub has_pin: bool,
}

/// A registration invitation created by a user with `users.manage`. After the
/// bootstrap owner, an invite is the only way to create an account.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Invite {
    pub token: String,
    /// Permissions the invited account will be granted.
    pub permissions: Vec<Permission>,
    #[serde(rename = "createdBy", skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    /// Unix-seconds expiry.
    #[serde(rename = "expiresAt")]
    pub expires_at: i64,
    pub used: bool,
}
