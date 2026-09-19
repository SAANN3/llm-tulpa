mod entities;

use axum::http::StatusCode;
use entities::users;
use sea_orm::{prelude::*, ActiveValue::Set, DatabaseConnection, DbBackend, QueryOrder, Statement};

use crate::services::error::ErrorService;

pub const ROLE_OWNER: &str = "owner";
pub const ROLE_USER: &str = "user";

/// Owns the `users` table and password hashing/verification (bcrypt). Also seeds the 1:1
/// `user_settings` row on user creation so per-user settings always have a home. The
/// `users` SeaORM entity is private to this module; callers only ever see the plain
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

    /// Creates a user with a bcrypt-hashed password and seeds its empty settings row.
    /// Errs `AlreadyExists` on a duplicate username.
    pub async fn create_user(&self, username: &str, password: &str, role: &str) -> Result<User, UserStoreErrors> {
        if users::Entity::find()
            .filter(users::Column::Username.eq(username))
            .one(&self.db)
            .await?
            .is_some()
        {
            return Err(UserStoreErrors::AlreadyExists);
        }

        let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)?;

        let model = users::ActiveModel {
            username: Set(username.to_string()),
            password_hash: Set(hash),
            role: Set(role.to_string()),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;

        self.db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO user_settings (user_id) VALUES ($1)",
                [model.id.into()],
            ))
            .await?;

        Ok(Self::to_user(model))
    }

    /// Verifies a username/password pair, returning the user on success and `None` on any
    /// mismatch (unknown username or wrong password — deliberately indistinguishable).
    pub async fn verify_login(&self, username: &str, password: &str) -> Result<Option<User>, UserStoreErrors> {
        let Some(model) = users::Entity::find()
            .filter(users::Column::Username.eq(username))
            .one(&self.db)
            .await?
        else {
            return Ok(None);
        };

        if bcrypt::verify(password, &model.password_hash)? {
            Ok(Some(Self::to_user(model)))
        } else {
            Ok(None)
        }
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
#[derive(serde::Serialize, Clone)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub created_at: DateTimeUtc,
}

#[derive(Debug)]
pub enum UserStoreErrors {
    QueryFailed(DbErr),
    Hash(bcrypt::BcryptError),
    AlreadyExists,
}

impl From<DbErr> for UserStoreErrors {
    fn from(err: DbErr) -> Self {
        UserStoreErrors::QueryFailed(err)
    }
}

impl From<bcrypt::BcryptError> for UserStoreErrors {
    fn from(err: bcrypt::BcryptError) -> Self {
        UserStoreErrors::Hash(err)
    }
}

impl From<UserStoreErrors> for ErrorService {
    fn from(err: UserStoreErrors) -> Self {
        match err {
            UserStoreErrors::QueryFailed(e) => {
                tracing::error!("user store query failed: {e}");
                ErrorService::internal("database query failed")
            }
            UserStoreErrors::Hash(e) => {
                tracing::error!("password hashing failed: {e}");
                ErrorService::internal("password hashing failed")
            }
            UserStoreErrors::AlreadyExists => {
                ErrorService::new(StatusCode::CONFLICT, "a user with that name already exists")
            }
        }
    }
}
