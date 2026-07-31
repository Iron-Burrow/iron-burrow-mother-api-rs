use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use super::errors::RepositoryError;
use crate::domain::capabilities::Capability;

const EMAIL_TOKEN_TTL_SECONDS: i64 = 15 * 60;
const SESSION_ABSOLUTE_TTL_SECONDS: i64 = 8 * 60 * 60;
const SESSION_IDLE_TTL_SECONDS: i64 = 30 * 60;
const DEMO_INTENT_TTL_SECONDS: i64 = 10 * 60;
const DEMO_KEY_TTL_SECONDS: i64 = 24 * 60 * 60;

#[derive(Clone, Debug)]
pub(crate) struct AccountRepository(pub(crate) PgPool);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AccountEntry {
    pub(crate) email_normalized: String,
    pub(crate) public_id: String,
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

    pub(crate) async fn request_entry(
        &self,
        email_normalized: &str,
        email_lookup_hash: &[u8],
        token_hash: &[u8],
        purpose: &str,
    ) -> Result<AccountEntry, RepositoryError> {
        let mut tx = self.0.begin().await.map_err(RepositoryError::new)?;
        let existing = sqlx::query_as::<_, IdentityRow>(
            "select identity.id as identity_id, identity.email_normalized, account.public_id, identity.status as identity_status from mother_api.account_identity identity join mother_api.ib_account account on account.id = identity.ib_account_id where identity.email_lookup_hash = $1 for update",
        )
        .bind(email_lookup_hash)
        .fetch_optional(&mut *tx)
        .await
        .map_err(RepositoryError::new)?;

        let (identity_id, entry) = match existing {
            Some(row) => (
                row.identity_id,
                AccountEntry {
                    email_normalized: row.email_normalized,
                    public_id: row.public_id,
                },
            ),
            None => {
                let account_id = Uuid::new_v4();
                let public_id = format!("iba_{}", account_id.simple());
                sqlx::query("insert into mother_api.ib_account (id, public_id) values ($1, $2)")
                    .bind(account_id)
                    .bind(&public_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(RepositoryError::new)?;
                let identity_id = Uuid::new_v4();
                sqlx::query("insert into mother_api.account_identity (id, ib_account_id, email_normalized, email_lookup_hash) values ($1, $2, $3, $4)")
                    .bind(identity_id).bind(account_id).bind(email_normalized).bind(email_lookup_hash).execute(&mut *tx).await.map_err(RepositoryError::new)?;
                (
                    identity_id,
                    AccountEntry {
                        email_normalized: email_normalized.to_string(),
                        public_id,
                    },
                )
            }
        };

        sqlx::query("update mother_api.email_verification set revoked_at = now() where account_identity_id = $1 and consumed_at is null and revoked_at is null")
            .bind(identity_id).execute(&mut *tx).await.map_err(RepositoryError::new)?;
        sqlx::query("insert into mother_api.email_verification (account_identity_id, purpose, secret_hash, expires_at) values ($1, $2, $3, now() + make_interval(secs => $4))")
            .bind(identity_id).bind(purpose).bind(token_hash).bind(EMAIL_TOKEN_TTL_SECONDS).execute(&mut *tx).await.map_err(RepositoryError::new)?;
        tx.commit().await.map_err(RepositoryError::new)?;
        Ok(entry)
    }

    pub(crate) async fn consume_entry_and_create_session(
        &self,
        token_hash: &[u8],
        session_hash: &[u8],
        csrf_hash: &[u8],
    ) -> Result<Option<String>, RepositoryError> {
        let mut tx = self.0.begin().await.map_err(RepositoryError::new)?;
        let row = sqlx::query_as::<_, EntryTokenRow>(
            "select verification.id as verification_id, verification.purpose, identity.id as identity_id, account.id as account_id, account.public_id, identity.status as identity_status, account.status as account_status from mother_api.email_verification verification join mother_api.account_identity identity on identity.id = verification.account_identity_id join mother_api.ib_account account on account.id = identity.ib_account_id where verification.secret_hash = $1 and verification.consumed_at is null and verification.revoked_at is null and verification.expires_at > now() for update of verification, identity, account",
        ).bind(token_hash).fetch_optional(&mut *tx).await.map_err(RepositoryError::new)?;
        let Some(row) = row else {
            return Ok(None);
        };
        if row.account_status == "suspended"
            || row.account_status == "closed"
            || (row.purpose == "login" && row.identity_status != "verified")
        {
            return Ok(None);
        }
        sqlx::query("update mother_api.email_verification set consumed_at = now() where id = $1")
            .bind(row.verification_id)
            .execute(&mut *tx)
            .await
            .map_err(RepositoryError::new)?;
        sqlx::query("update mother_api.account_identity set status = 'verified', verified_at = coalesce(verified_at, now()), updated_at = now() where id = $1").bind(row.identity_id).execute(&mut *tx).await.map_err(RepositoryError::new)?;
        sqlx::query("update mother_api.ib_account set status = 'active', updated_at = now() where id = $1 and status = 'pending_verification'").bind(row.account_id).execute(&mut *tx).await.map_err(RepositoryError::new)?;
        for capability in Capability::LEGACY_BASELINE {
            sqlx::query("insert into mother_api.ib_account_capability_grant (ib_account_id, capability_id, network_scope) values ($1, $2, '*') on conflict do nothing")
                .bind(row.account_id).bind(capability.id()).execute(&mut *tx).await.map_err(RepositoryError::new)?;
        }
        sqlx::query("update mother_api.browser_session set revoked_at = now() where ib_account_id = $1 and revoked_at is null")
            .bind(row.account_id).execute(&mut *tx).await.map_err(RepositoryError::new)?;
        sqlx::query("insert into mother_api.browser_session (ib_account_id, account_identity_id, session_hash, csrf_hash, expires_at, idle_expires_at) values ($1, $2, $3, $4, now() + make_interval(secs => $5), now() + make_interval(secs => $6))")
            .bind(row.account_id).bind(row.identity_id).bind(session_hash).bind(csrf_hash).bind(SESSION_ABSOLUTE_TTL_SECONDS).bind(SESSION_IDLE_TTL_SECONDS).execute(&mut *tx).await.map_err(RepositoryError::new)?;
        tx.commit().await.map_err(RepositoryError::new)?;
        Ok(Some(row.public_id))
    }

    pub(crate) async fn find_session(
        &self,
        session_hash: &[u8],
    ) -> Result<Option<BrowserSessionLookup>, RepositoryError> {
        let row = sqlx::query_as::<_, SessionRow>(
            "update mother_api.browser_session session set last_seen_at = now(), idle_expires_at = least(session.expires_at, now() + make_interval(secs => $2)) from mother_api.ib_account account where session.ib_account_id = account.id and session.session_hash = $1 and session.revoked_at is null and session.expires_at > now() and session.idle_expires_at > now() and account.status = 'active' returning session.ib_account_id, account.public_id, session.csrf_hash",
        ).bind(session_hash).bind(SESSION_IDLE_TTL_SECONDS).fetch_optional(&self.0).await.map_err(RepositoryError::new)?;
        Ok(row.map(Into::into))
    }

    pub(crate) async fn revoke_session(&self, session_hash: &[u8]) -> Result<(), RepositoryError> {
        sqlx::query("update mother_api.browser_session set revoked_at = now() where session_hash = $1 and revoked_at is null")
            .bind(session_hash).execute(&self.0).await.map_err(RepositoryError::new)?;
        Ok(())
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

#[derive(FromRow)]
struct IdentityRow {
    identity_id: Uuid,
    email_normalized: String,
    public_id: String,
    #[allow(dead_code)]
    identity_status: String,
}
#[derive(FromRow)]
struct EntryTokenRow {
    verification_id: Uuid,
    purpose: String,
    identity_id: Uuid,
    account_id: Uuid,
    public_id: String,
    identity_status: String,
    account_status: String,
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
