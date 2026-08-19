use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use super::errors::RepositoryError;
use crate::domain::capabilities::Capability;

const SESSION_ABSOLUTE_TTL_SECONDS: i64 = 8 * 60 * 60;
const SESSION_IDLE_TTL_SECONDS: i64 = 30 * 60;
const DEMO_INTENT_TTL_SECONDS: i64 = 10 * 60;
const DEMO_KEY_TTL_SECONDS: i64 = 24 * 60 * 60;
const INITIAL_WORKSPACE_NAME: &str = "Personal Workspace";

#[derive(Clone, Debug)]
pub(crate) struct AccountRepository(pub(crate) PgPool);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SignupOutcome {
    Created,
    AlreadyRegistered,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PasswordLoginIdentity {
    pub(crate) account_identity_id: Uuid,
    pub(crate) password_hash: String,
    pub(crate) account_status: String,
    pub(crate) identity_status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BrowserSessionLookup {
    pub(crate) ib_account_id: Uuid,
    pub(crate) public_id: String,
    pub(crate) csrf_hash: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IssuedDemoKey {
    pub(crate) api_key_id: Uuid,
    pub(crate) expires_at: String,
}

impl AccountRepository {
    pub(crate) fn database(pool: PgPool) -> Self {
        Self(pool)
    }

    pub(crate) async fn signup_and_create_session(
        &self,
        email_normalized: &str,
        email_lookup_hash: &[u8],
        password_hash: &str,
        session_hash: &[u8],
        csrf_hash: &[u8],
    ) -> Result<SignupOutcome, RepositoryError> {
        let mut tx = self.0.begin().await.map_err(RepositoryError::new)?;
        let existing = sqlx::query_scalar::<_, Uuid>(
            "select id from mother_api.account_identity where email_lookup_hash = $1 for update",
        )
        .bind(email_lookup_hash)
        .fetch_optional(&mut *tx)
        .await
        .map_err(RepositoryError::new)?;

        if existing.is_some() {
            tx.rollback().await.map_err(RepositoryError::new)?;
            return Ok(SignupOutcome::AlreadyRegistered);
        }

        let account_id = Uuid::new_v4();
        let public_id = format!("iba_{}", account_id.simple());
        sqlx::query(
            "insert into mother_api.ib_account (id, public_id, status) values ($1, $2, 'active')",
        )
        .bind(account_id)
        .bind(&public_id)
        .execute(&mut *tx)
        .await
        .map_err(RepositoryError::new)?;
        let identity_id = Uuid::new_v4();
        sqlx::query("insert into mother_api.account_identity (id, ib_account_id, email_normalized, email_lookup_hash, password_hash) values ($1, $2, $3, $4, $5)")
            .bind(identity_id).bind(account_id).bind(email_normalized).bind(email_lookup_hash).bind(password_hash).execute(&mut *tx).await.map_err(RepositoryError::new)?;
        grant_baseline_capabilities(&mut tx, account_id).await?;
        create_initial_workspace(&mut tx, account_id).await?;
        create_session(&mut tx, account_id, identity_id, session_hash, csrf_hash).await?;
        tx.commit().await.map_err(RepositoryError::new)?;
        Ok(SignupOutcome::Created)
    }

    pub(crate) async fn find_password_login_identity(
        &self,
        email_lookup_hash: &[u8],
    ) -> Result<Option<PasswordLoginIdentity>, RepositoryError> {
        sqlx::query_as::<_, PasswordLoginIdentityRow>(
            "select identity.id as account_identity_id, identity.password_hash, account.status as account_status, identity.status as identity_status from mother_api.account_identity identity join mother_api.ib_account account on account.id = identity.ib_account_id where identity.email_lookup_hash = $1",
        )
        .bind(email_lookup_hash)
        .fetch_optional(&self.0)
        .await
        .map_err(RepositoryError::new)
        .map(|row| row.map(Into::into))
    }

    pub(crate) async fn create_password_login_session(
        &self,
        identity_id: Uuid,
        session_hash: &[u8],
        csrf_hash: &[u8],
    ) -> Result<bool, RepositoryError> {
        let mut tx = self.0.begin().await.map_err(RepositoryError::new)?;
        let account_id = sqlx::query_scalar::<_, Uuid>(
            "select account.id from mother_api.account_identity identity join mother_api.ib_account account on account.id = identity.ib_account_id where identity.id = $1 and identity.status <> 'disabled' and identity.password_hash is not null and account.status = 'active' for update of identity, account",
        )
        .bind(identity_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(RepositoryError::new)?;
        let Some(account_id) = account_id else {
            tx.rollback().await.map_err(RepositoryError::new)?;
            return Ok(false);
        };
        create_session(&mut tx, account_id, identity_id, session_hash, csrf_hash).await?;
        tx.commit().await.map_err(RepositoryError::new)?;
        Ok(true)
    }

    pub(crate) async fn update_password_hash(
        &self,
        identity_id: Uuid,
        password_hash: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query("update mother_api.account_identity set password_hash = $2, updated_at = now() where id = $1")
            .bind(identity_id)
            .bind(password_hash)
            .execute(&self.0)
            .await
            .map_err(RepositoryError::new)?;
        Ok(())
    }

    pub(crate) async fn find_session(
        &self,
        session_hash: &[u8],
    ) -> Result<Option<BrowserSessionLookup>, RepositoryError> {
        let row = sqlx::query_as::<_, SessionRow>(
            "update mother_api.browser_session session set last_seen_at = now(), idle_expires_at = least(session.expires_at, now() + make_interval(secs => $2)) from mother_api.ib_account account, mother_api.account_identity identity where session.ib_account_id = account.id and identity.id = session.account_identity_id and session.session_hash = $1 and session.revoked_at is null and session.expires_at > now() and session.idle_expires_at > now() and account.status = 'active' and identity.status <> 'disabled' returning session.ib_account_id, account.public_id, session.csrf_hash",
        ).bind(session_hash).bind(SESSION_IDLE_TTL_SECONDS).fetch_optional(&self.0).await.map_err(RepositoryError::new)?;
        Ok(row.map(Into::into))
    }

    pub(crate) async fn revoke_session(&self, session_hash: &[u8]) -> Result<(), RepositoryError> {
        sqlx::query("update mother_api.browser_session set revoked_at = now() where session_hash = $1 and revoked_at is null")
            .bind(session_hash).execute(&self.0).await.map_err(RepositoryError::new)?;
        Ok(())
    }

    pub(crate) async fn has_active_capability(
        &self,
        account_id: Uuid,
        capability: Capability,
        network_slug: &str,
    ) -> Result<bool, RepositoryError> {
        sqlx::query_scalar::<_, bool>(
            "select exists (select 1 from mother_api.ib_account_capability_grant where ib_account_id = $1 and capability_id = $2 and status = 'active' and revoked_at is null and (expires_at is null or expires_at > now()) and network_scope in ('*', $3))",
        )
        .bind(account_id)
        .bind(capability.id())
        .bind(network_slug)
        .fetch_one(&self.0)
        .await
        .map_err(RepositoryError::new)
    }

    pub(crate) async fn create_demo_intent(
        &self,
        secret_hash: &[u8],
    ) -> Result<(), RepositoryError> {
        sqlx::query("insert into mother_api.anonymous_demo_issuance_intent (secret_hash, expires_at) values ($1, now() + make_interval(secs => $2))")
            .bind(secret_hash).bind(DEMO_INTENT_TTL_SECONDS).execute(&self.0).await.map_err(RepositoryError::new)?;
        Ok(())
    }

    pub(crate) async fn consume_demo_intent_and_issue_key(
        &self,
        intent_hash: &[u8],
        key_prefix: &str,
        key_hash: &[u8],
    ) -> Result<Option<IssuedDemoKey>, RepositoryError> {
        let mut tx = self.0.begin().await.map_err(RepositoryError::new)?;
        let intent_id = sqlx::query_scalar::<_, Uuid>("select id from mother_api.anonymous_demo_issuance_intent where secret_hash = $1 and consumed_at is null and expires_at > now() for update")
            .bind(intent_hash).fetch_optional(&mut *tx).await.map_err(RepositoryError::new)?;
        let Some(intent_id) = intent_id else {
            return Ok(None);
        };
        let row = sqlx::query_as::<_, DemoKeyRow>("insert into mother_api.api_key (kind, label, key_prefix, key_hash, expires_at) values ('anonymous_demo', 'anonymous demo', $1, $2, now() + make_interval(secs => $3)) returning id as api_key_id, expires_at::text")
            .bind(key_prefix).bind(key_hash).bind(DEMO_KEY_TTL_SECONDS).fetch_one(&mut *tx).await.map_err(RepositoryError::new)?;
        sqlx::query("insert into mother_api.api_key_policy (api_key_id, requests_per_minute, requests_per_day) values ($1, 10, 100)").bind(row.api_key_id).execute(&mut *tx).await.map_err(RepositoryError::new)?;
        for capability in Capability::LEGACY_BASELINE {
            sqlx::query("insert into mother_api.api_key_capability_grant (api_key_id, capability_id, network_scope) values ($1, $2, 'eth-mainnet')")
                .bind(row.api_key_id).bind(capability.id()).execute(&mut *tx).await.map_err(RepositoryError::new)?;
        }
        sqlx::query("update mother_api.anonymous_demo_issuance_intent set consumed_at = now(), api_key_id = $2 where id = $1").bind(intent_id).bind(row.api_key_id).execute(&mut *tx).await.map_err(RepositoryError::new)?;
        tx.commit().await.map_err(RepositoryError::new)?;
        Ok(Some(IssuedDemoKey {
            api_key_id: row.api_key_id,
            expires_at: row.expires_at,
        }))
    }
}

async fn grant_baseline_capabilities(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: Uuid,
) -> Result<(), RepositoryError> {
    for capability in Capability::ACCOUNT_BASELINE
        .into_iter()
        .chain(Capability::DATALAB_BROWSER_BASELINE)
    {
        sqlx::query("insert into mother_api.ib_account_capability_grant (ib_account_id, capability_id, network_scope) values ($1, $2, '*') on conflict do nothing")
            .bind(account_id).bind(capability.id()).execute(&mut **tx).await.map_err(RepositoryError::new)?;
    }
    Ok(())
}

async fn create_initial_workspace(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: Uuid,
) -> Result<(), RepositoryError> {
    let workspace_id = Uuid::new_v4();
    let workspace_public_id = format!("wsp_{}", workspace_id.simple());
    sqlx::query(
        "insert into mother_api.workspace (id, public_id, owner_ib_account_id, name) values ($1, $2, $3, $4)",
    )
    .bind(workspace_id)
    .bind(&workspace_public_id)
    .bind(account_id)
    .bind(INITIAL_WORKSPACE_NAME)
    .execute(&mut **tx)
    .await
    .map_err(RepositoryError::new)?;

    let event_id = Uuid::new_v4();
    sqlx::query(
        "insert into mother_api.workspace_activity_event (id, public_id, workspace_id, event_type, actor_kind, payload_version, payload) values ($1, $2, $3, 'workspace.created', 'browser_session', 1, $4)",
    )
    .bind(event_id)
    .bind(format!("wae_{}", event_id.simple()))
    .bind(workspace_id)
    .bind(serde_json::json!({"workspace": {"name": INITIAL_WORKSPACE_NAME, "description": null}}))
    .execute(&mut **tx)
    .await
    .map_err(RepositoryError::new)?;
    Ok(())
}

async fn create_session(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: Uuid,
    identity_id: Uuid,
    session_hash: &[u8],
    csrf_hash: &[u8],
) -> Result<(), RepositoryError> {
    sqlx::query("update mother_api.browser_session set revoked_at = now() where ib_account_id = $1 and revoked_at is null")
        .bind(account_id).execute(&mut **tx).await.map_err(RepositoryError::new)?;
    sqlx::query("insert into mother_api.browser_session (ib_account_id, account_identity_id, session_hash, csrf_hash, expires_at, idle_expires_at) values ($1, $2, $3, $4, now() + make_interval(secs => $5), now() + make_interval(secs => $6))")
        .bind(account_id).bind(identity_id).bind(session_hash).bind(csrf_hash).bind(SESSION_ABSOLUTE_TTL_SECONDS).bind(SESSION_IDLE_TTL_SECONDS).execute(&mut **tx).await.map_err(RepositoryError::new)?;
    Ok(())
}

#[derive(FromRow)]
struct PasswordLoginIdentityRow {
    account_identity_id: Uuid,
    password_hash: Option<String>,
    account_status: String,
    identity_status: String,
}
impl From<PasswordLoginIdentityRow> for PasswordLoginIdentity {
    fn from(row: PasswordLoginIdentityRow) -> Self {
        Self {
            account_identity_id: row.account_identity_id,
            password_hash: row.password_hash.unwrap_or_default(),
            account_status: row.account_status,
            identity_status: row.identity_status,
        }
    }
}
#[derive(FromRow)]
struct SessionRow {
    ib_account_id: Uuid,
    public_id: String,
    csrf_hash: Vec<u8>,
}
impl From<SessionRow> for BrowserSessionLookup {
    fn from(row: SessionRow) -> Self {
        Self {
            ib_account_id: row.ib_account_id,
            public_id: row.public_id,
            csrf_hash: row.csrf_hash,
        }
    }
}
#[derive(FromRow)]
struct DemoKeyRow {
    api_key_id: Uuid,
    expires_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::passwords, test_utils::postgres::migrated_pool};

    async fn remove_identity(pool: &PgPool, lookup_hash: &[u8]) {
        let Some(account_id) = sqlx::query_scalar::<_, Uuid>(
            "select ib_account_id from mother_api.account_identity where email_lookup_hash = $1",
        )
        .bind(lookup_hash)
        .fetch_optional(pool)
        .await
        .unwrap() else {
            return;
        };
        sqlx::query("delete from mother_api.browser_session where ib_account_id = $1")
            .bind(account_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("delete from mother_api.workspace where owner_ib_account_id = $1")
            .bind(account_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("delete from mother_api.account_identity where email_lookup_hash = $1")
            .bind(lookup_hash)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("delete from mother_api.ib_account where id = $1")
            .bind(account_id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn password_signup_creates_an_active_account_grants_and_rotating_session() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        let lookup_hash = [42_u8; 32];
        remove_identity(&pool, &lookup_hash).await;
        let repository = AccountRepository::database(pool.clone());
        let password_hash = passwords::hash("correct horse battery staple").unwrap();

        assert_eq!(
            repository
                .signup_and_create_session(
                    "password-signup@example.test",
                    &lookup_hash,
                    &password_hash,
                    &[1_u8; 32],
                    &[2_u8; 32],
                )
                .await
                .unwrap(),
            SignupOutcome::Created
        );
        assert_eq!(
            repository
                .signup_and_create_session(
                    "password-signup@example.test",
                    &lookup_hash,
                    &password_hash,
                    &[3_u8; 32],
                    &[4_u8; 32],
                )
                .await
                .unwrap(),
            SignupOutcome::AlreadyRegistered
        );

        let row = sqlx::query_as::<_, (Uuid, String, Option<String>, Option<String>, i64, i64, String)>(
            "select account.id, account.status, identity.password_hash, identity.verified_at::text, (select count(*) from mother_api.ib_account_capability_grant capability_grant where capability_grant.ib_account_id = account.id), (select count(*) from mother_api.workspace workspace where workspace.owner_ib_account_id = account.id), (select workspace.name from mother_api.workspace workspace where workspace.owner_ib_account_id = account.id) from mother_api.ib_account account join mother_api.account_identity identity on identity.ib_account_id = account.id where identity.email_lookup_hash = $1",
        )
        .bind(lookup_hash.as_slice())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.1, "active");
        assert_eq!(row.2.as_deref(), Some(password_hash.as_str()));
        assert_eq!(row.3, None);
        assert_eq!(row.4, 9);
        assert_eq!(row.5, 1);
        assert_eq!(row.6, INITIAL_WORKSPACE_NAME);
        assert!(repository
            .find_session(&[1_u8; 32])
            .await
            .unwrap()
            .is_some());

        let identity = repository
            .find_password_login_identity(&lookup_hash)
            .await
            .unwrap()
            .unwrap();
        assert!(passwords::verify(
            "correct horse battery staple",
            &identity.password_hash
        ));
        assert!(repository
            .create_password_login_session(identity.account_identity_id, &[5_u8; 32], &[6_u8; 32])
            .await
            .unwrap());
        assert!(repository
            .find_session(&[1_u8; 32])
            .await
            .unwrap()
            .is_none());
        assert!(repository
            .find_session(&[5_u8; 32])
            .await
            .unwrap()
            .is_some());

        sqlx::query("update mother_api.account_identity set status = 'disabled' where email_lookup_hash = $1")
            .bind(lookup_hash.as_slice())
            .execute(&pool)
            .await
            .unwrap();
        assert!(repository
            .find_session(&[5_u8; 32])
            .await
            .unwrap()
            .is_none());
        remove_identity(&pool, &lookup_hash).await;
    }
}
