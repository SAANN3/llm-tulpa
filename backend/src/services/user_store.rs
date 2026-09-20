mod entities;

use axum::http::StatusCode;
use entities::users;
use sea_orm::{prelude::*, ActiveValue::Set, DatabaseConnection, QueryOrder, SqlErr};

use crate::services::auth::{AuthErrors, AuthService};
use crate::services::error::ErrorService;

pub const ROLE_OWNER: &str = "owner";
pub const ROLE_USER: &str = "user";

/// The partial unique index (see `services::migrate`) that lets the table hold at most one owner.
const OWNER_UNIQUE_INDEX: &str = "users_one_owner";

/// Owns the `users` table. Password hashing lives in `AuthService`. The `users` SeaORM entity is private to this module; callers only ever see the plain
/// `User` struct below (which never carries the password hash).
pub struct UserStore {
    db: DatabaseConnection,
}

impl UserStore {
    /// Holds an already-connected, already-migrated connection (see `services::bootstrap`).
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// How many accounts exist — the setup wizard uses `0` to mean "no owner yet, allow
    /// unauthenticated owner creation".
    pub async fn user_count(&self) -> Result<u64, UserStoreErrors> {
        Ok(users::Entity::find().count(&self.db).await?)
    }

    /// Creates a user with a bcrypt-hashed password. Uniqueness is enforced by the database
    /// (unique username, and at most one owner), not by a look-before-you-insert check, so two
    /// simultaneous requests can't both win: the loser gets `AlreadyExists` / `OwnerExists`.
    /// The user's `user_settings` row is created lazily by `SettingsStore` on first use.
    pub async fn create_user(&self, username: &str, password: &str, role: &str) -> Result<User, UserStoreErrors> {
        let hash = AuthService::hash_password(password).await?;

        let model = users::ActiveModel {
            username: Set(username.to_string()),
            password_hash: Set(hash),
            role: Set(role.to_string()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .map_err(|e| match e.sql_err() {
            Some(SqlErr::UniqueConstraintViolation(detail)) if detail.contains(OWNER_UNIQUE_INDEX) => {
                UserStoreErrors::OwnerExists
            }
            Some(SqlErr::UniqueConstraintViolation(_)) => UserStoreErrors::AlreadyExists,
            _ => UserStoreErrors::QueryFailed(e),
        })?;

        Ok(Self::to_user(model))
    }

    /// Verifies a username/password pair, returning the user on success and `None` on any
    /// mismatch (unknown username or wrong password — deliberately indistinguishable, timing
    /// included: an unknown username still pays for one bcrypt verification).
    pub async fn verify_login(&self, username: &str, password: &str) -> Result<Option<User>, UserStoreErrors> {
        let found = users::Entity::find()
            .filter(users::Column::Username.eq(username))
            .one(&self.db)
            .await?;

        let hash = match &found {
            Some(model) => model.password_hash.clone(),
            None => AuthService::dummy_hash().to_string(),
        };
        let matches = AuthService::verify_password(password, hash).await?;

        Ok(found.filter(|_| matches).map(Self::to_user))
    }

    /// The owner's id, if the owner account exists yet.
    pub async fn owner_id(&self) -> Result<Option<i64>, UserStoreErrors> {
        Ok(users::Entity::find()
            .filter(users::Column::Role.eq(ROLE_OWNER))
            .one(&self.db)
            .await?
            .map(|owner| owner.id))
    }

    /// A user by id — `None` if it doesn't exist (e.g. a token for a since-deleted user).
    pub async fn get(&self, id: i64) -> Result<Option<User>, UserStoreErrors> {
        Ok(users::Entity::find_by_id(id).one(&self.db).await?.map(Self::to_user))
    }

    /// Every user, oldest first — the owner's user-management list.
    pub async fn list_users(&self) -> Result<Vec<User>, UserStoreErrors> {
        Ok(users::Entity::find()
            .order_by_asc(users::Column::Id)
            .all(&self.db)
            .await?
            .into_iter()
            .map(Self::to_user)
            .collect())
    }

    /// Deletes a user (cascading to its chats/settings/etc. via FK). Not an error if it
    /// was already gone.
    pub async fn delete_user(&self, id: i64) -> Result<(), UserStoreErrors> {
        users::Entity::delete_by_id(id).exec(&self.db).await?;
        Ok(())
    }

    fn to_user(model: users::Model) -> User {
        User {
            id: model.id,
            username: model.username,
            role: model.role,
            created_at: model.created_at,
        }
    }
}

/// The user shape the rest of the app sees — no password hash.
#[derive(serde::Serialize, utoipa::ToSchema, Clone)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub role: String,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
}

#[derive(Debug)]
pub enum UserStoreErrors {
    QueryFailed(DbErr),
    Auth(AuthErrors),
    AlreadyExists,
    OwnerExists,
}

impl From<DbErr> for UserStoreErrors {
    fn from(err: DbErr) -> Self {
        UserStoreErrors::QueryFailed(err)
    }
}

impl From<AuthErrors> for UserStoreErrors {
    fn from(err: AuthErrors) -> Self {
        UserStoreErrors::Auth(err)
    }
}

impl From<UserStoreErrors> for ErrorService {
    fn from(err: UserStoreErrors) -> Self {
        match err {
            UserStoreErrors::QueryFailed(e) => {
                tracing::error!("user store query failed: {e}");
                ErrorService::internal("database query failed")
            }
            UserStoreErrors::Auth(e) => e.into(),
            UserStoreErrors::OwnerExists => {
                ErrorService::new(StatusCode::CONFLICT, "an owner account already exists")
            }
            UserStoreErrors::AlreadyExists => {
                ErrorService::new(StatusCode::CONFLICT, "a user with that name already exists")
            }
        }
    }
}
