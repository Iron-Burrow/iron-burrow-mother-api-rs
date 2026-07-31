use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::errors::RepositoryError;

pub(crate) const BALANCE_NETWORKS: [&str; 2] = ["eth-mainnet", "base-mainnet"];

#[derive(Clone, Debug)]
pub(crate) struct WorkspaceRepository(pub(crate) PgPool);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Workspace {
    pub(crate) id: Uuid,
    pub(crate) public_id: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) status: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WorkspaceMemberAddress {
    pub(crate) id: Uuid,
    pub(crate) public_id: String,
    pub(crate) network_slug: String,
    pub(crate) address: String,
    pub(crate) client_ref: Option<String>,
    pub(crate) labels: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct WorkspaceActivityEvent {
    pub(crate) public_id: String,
    pub(crate) event_type: String,
    pub(crate) actor_kind: String,
    pub(crate) payload_version: i32,
    pub(crate) payload: Value,
    pub(crate) occurred_at: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AddMemberOutcome {
    Added,
    AlreadyPresent,
    Inactive,
    LimitReached,
}

impl WorkspaceRepository {
    pub(crate) fn database(pool: PgPool) -> Self {
        Self(pool)
    }

    pub(crate) async fn list_active(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<Workspace>, RepositoryError> {
        sqlx::query_as::<_, WorkspaceRow>("select id, public_id, name, description, status from mother_api.workspace where owner_ib_account_id = $1 and status = 'active' order by updated_at desc")
            .bind(account_id).fetch_all(&self.0).await.map_err(RepositoryError::new).map(|rows| rows.into_iter().map(Into::into).collect())
    }

    pub(crate) async fn create(
        &self,
        account_id: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> Result<Workspace, RepositoryError> {
        let id = Uuid::new_v4();
        let public_id = format!("wsp_{}", id.simple());
        let mut tx = self.0.begin().await.map_err(RepositoryError::new)?;
        let workspace = sqlx::query_as::<_, WorkspaceRow>("insert into mother_api.workspace (id, public_id, owner_ib_account_id, name, description) values ($1, $2, $3, $4, $5) returning id, public_id, name, description, status")
            .bind(id).bind(public_id).bind(account_id).bind(name).bind(description).fetch_one(&mut *tx).await.map_err(RepositoryError::new)?;
        append_event(
            &mut tx,
            id,
            "workspace.created",
            json!({"workspace": {"name": name, "description": description}}),
        )
        .await?;
        tx.commit().await.map_err(RepositoryError::new)?;
        Ok(workspace.into())
    }

    pub(crate) async fn find_owned(
        &self,
        account_id: Uuid,
        public_id: &str,
    ) -> Result<Option<Workspace>, RepositoryError> {
        sqlx::query_as::<_, WorkspaceRow>("select id, public_id, name, description, status from mother_api.workspace where owner_ib_account_id = $1 and public_id = $2")
            .bind(account_id).bind(public_id).fetch_optional(&self.0).await.map_err(RepositoryError::new).map(|row| row.map(Into::into))
    }

    pub(crate) async fn rename(
        &self,
        account_id: Uuid,
        public_id: &str,
        name: &str,
    ) -> Result<bool, RepositoryError> {
        let mut tx = self.0.begin().await.map_err(RepositoryError::new)?;
        let id = sqlx::query_scalar::<_, Uuid>("update mother_api.workspace set name = $3, updated_at = now() where owner_ib_account_id = $1 and public_id = $2 and status = 'active' returning id")
            .bind(account_id).bind(public_id).bind(name).fetch_optional(&mut *tx).await.map_err(RepositoryError::new)?;
        if let Some(id) = id {
            append_event(
                &mut tx,
                id,
                "workspace.renamed",
                json!({"workspace": {"name": name}}),
            )
            .await?;
        }
        tx.commit().await.map_err(RepositoryError::new)?;
        Ok(id.is_some())
    }

    pub(crate) async fn set_archived(
        &self,
        account_id: Uuid,
        public_id: &str,
        archived: bool,
    ) -> Result<bool, RepositoryError> {
        let target = if archived { "archived" } else { "active" };
        let mut tx = self.0.begin().await.map_err(RepositoryError::new)?;
        let id = sqlx::query_scalar::<_, Uuid>("update mother_api.workspace set status = $3, archived_at = case when $3 = 'archived' then now() else null end, updated_at = now() where owner_ib_account_id = $1 and public_id = $2 and status <> $3 returning id")
            .bind(account_id).bind(public_id).bind(target).fetch_optional(&mut *tx).await.map_err(RepositoryError::new)?;
        if let Some(id) = id {
            append_event(
                &mut tx,
                id,
                if archived {
                    "workspace.archived"
                } else {
                    "workspace.restored"
                },
                json!({"workspace": {"status": target}}),
            )
            .await?;
        }
        tx.commit().await.map_err(RepositoryError::new)?;
        Ok(id.is_some())
    }

    pub(crate) async fn members(
        &self,
        workspace_id: Uuid,
    ) -> Result<Vec<WorkspaceMemberAddress>, RepositoryError> {
        sqlx::query_as::<_, MemberRow>("select member.id, member.public_id, member.network_slug, member.address, member.client_ref, coalesce(array_agg(label.label order by label.label) filter (where label.label is not null), '{}') as labels from mother_api.workspace_member_address member left join mother_api.workspace_member_address_label label on label.member_address_id = member.id where member.workspace_id = $1 group by member.id order by member.created_at")
            .bind(workspace_id).fetch_all(&self.0).await.map_err(RepositoryError::new).map(|rows| rows.into_iter().map(Into::into).collect())
    }

    pub(crate) async fn find_owned_member(
        &self,
        account_id: Uuid,
        workspace_public_id: &str,
        member_public_id: &str,
    ) -> Result<Option<(Workspace, WorkspaceMemberAddress)>, RepositoryError> {
        let Some(workspace) = self.find_owned(account_id, workspace_public_id).await? else {
            return Ok(None);
        };
        let member = sqlx::query_as::<_, MemberRow>("select member.id, member.public_id, member.network_slug, member.address, member.client_ref, coalesce(array_agg(label.label order by label.label) filter (where label.label is not null), '{}') as labels from mother_api.workspace_member_address member left join mother_api.workspace_member_address_label label on label.member_address_id = member.id where member.workspace_id = $1 and member.public_id = $2 group by member.id")
            .bind(workspace.id).bind(member_public_id).fetch_optional(&self.0).await.map_err(RepositoryError::new)?;
        Ok(member.map(|member| (workspace, member.into())))
    }

    pub(crate) async fn add_member(
        &self,
        workspace_id: Uuid,
        network_slug: &str,
        address: &str,
        client_ref: Option<&str>,
        maximum_members: usize,
    ) -> Result<AddMemberOutcome, RepositoryError> {
        let mut transaction = self.0.begin().await.map_err(RepositoryError::new)?;
        let status = sqlx::query_scalar::<_, String>(
            "select status from mother_api.workspace where id = $1 for update",
        )
        .bind(workspace_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(RepositoryError::new)?;

        let outcome = if status.as_deref() != Some("active") {
            AddMemberOutcome::Inactive
        } else {
            let already_present = sqlx::query_scalar::<_, bool>(
                "select exists (select 1 from mother_api.workspace_member_address where workspace_id = $1 and network_slug = $2 and address = $3)",
            )
            .bind(workspace_id)
            .bind(network_slug)
            .bind(address)
            .fetch_one(&mut *transaction)
            .await
            .map_err(RepositoryError::new)?;

            if already_present {
                AddMemberOutcome::AlreadyPresent
            } else {
                let member_count = sqlx::query_scalar::<_, i64>(
                    "select count(*) from mother_api.workspace_member_address where workspace_id = $1",
                )
                .bind(workspace_id)
                .fetch_one(&mut *transaction)
                .await
                .map_err(RepositoryError::new)?;

                if member_count >= maximum_members as i64 {
                    AddMemberOutcome::LimitReached
                } else {
                    let id = Uuid::new_v4();
                    let public_id = format!("wma_{}", id.simple());
                    let result = sqlx::query("insert into mother_api.workspace_member_address (id, public_id, workspace_id, network_slug, address, client_ref) values ($1, $2, $3, $4, $5, $6) on conflict (workspace_id, network_slug, address) do nothing")
                        .bind(id).bind(&public_id).bind(workspace_id).bind(network_slug).bind(address).bind(client_ref).execute(&mut *transaction).await.map_err(RepositoryError::new)?;
                    if result.rows_affected() == 1 {
                        append_event(&mut transaction, workspace_id, "member_address.added", json!({"member": {"public_id": public_id, "network_slug": network_slug, "address": address, "client_ref": client_ref}})).await?;
                        AddMemberOutcome::Added
                    } else {
                        AddMemberOutcome::AlreadyPresent
                    }
                }
            }
        };

        transaction.commit().await.map_err(RepositoryError::new)?;
        Ok(outcome)
    }

    pub(crate) async fn add_label(
        &self,
        member_id: Uuid,
        label: &str,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.0.begin().await.map_err(RepositoryError::new)?;
        let workspace_id = sqlx::query_scalar::<_, Uuid>(
            "select workspace_id from mother_api.workspace_member_address where id = $1",
        )
        .bind(member_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(RepositoryError::new)?;
        let result = sqlx::query("insert into mother_api.workspace_member_address_label (member_address_id, label) values ($1, $2) on conflict do nothing")
            .bind(member_id).bind(label).execute(&mut *tx).await.map_err(RepositoryError::new)?;
        if result.rows_affected() == 1 {
            append_event(
                &mut tx,
                workspace_id,
                "member_address.label_added",
                json!({"member_address_id": member_id, "label": label}),
            )
            .await?;
        }
        tx.commit().await.map_err(RepositoryError::new)?;
        Ok(())
    }

    pub(crate) async fn remove_label(
        &self,
        member_id: Uuid,
        label: &str,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.0.begin().await.map_err(RepositoryError::new)?;
        let workspace_id = sqlx::query_scalar::<_, Uuid>(
            "select workspace_id from mother_api.workspace_member_address where id = $1",
        )
        .bind(member_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(RepositoryError::new)?;
        let result = sqlx::query("delete from mother_api.workspace_member_address_label where member_address_id = $1 and lower(label) = lower($2)")
            .bind(member_id).bind(label).execute(&mut *tx).await.map_err(RepositoryError::new)?;
        if result.rows_affected() == 1 {
            append_event(
                &mut tx,
                workspace_id,
                "member_address.label_removed",
                json!({"member_address_id": member_id, "label": label}),
            )
            .await?;
        }
        tx.commit().await.map_err(RepositoryError::new)?;
        Ok(())
    }

    pub(crate) async fn append_observation(
        &self,
        workspace_id: Uuid,
        event_type: &str,
        payload: Value,
    ) -> Result<(), RepositoryError> {
        let mut tx = self.0.begin().await.map_err(RepositoryError::new)?;
        append_event(&mut tx, workspace_id, event_type, payload).await?;
        tx.commit().await.map_err(RepositoryError::new)
    }

    pub(crate) async fn list_activity(
        &self,
        workspace_id: Uuid,
        before: Option<&str>,
        limit: i64,
    ) -> Result<Vec<WorkspaceActivityEvent>, RepositoryError> {
        sqlx::query_as::<_, ActivityRow>(r#"select public_id, event_type, actor_kind, payload_version, payload, occurred_at::text as occurred_at
            from mother_api.workspace_activity_event
            where workspace_id = $1 and ($2::text is null or (occurred_at, public_id) < (select occurred_at, public_id from mother_api.workspace_activity_event where workspace_id = $1 and public_id = $2))
            order by occurred_at desc, public_id desc limit $3"#)
            .bind(workspace_id).bind(before).bind(limit).fetch_all(&self.0).await.map_err(RepositoryError::new).map(|rows| rows.into_iter().map(Into::into).collect())
    }
}

async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
    event_type: &str,
    payload: Value,
) -> Result<(), RepositoryError> {
    let id = Uuid::new_v4();
    sqlx::query("insert into mother_api.workspace_activity_event (id, public_id, workspace_id, event_type, actor_kind, payload_version, payload) values ($1, $2, $3, $4, 'browser_session', 1, $5)")
        .bind(id).bind(format!("wae_{}", id.simple())).bind(workspace_id).bind(event_type).bind(payload).execute(&mut **tx).await.map_err(RepositoryError::new)?;
    Ok(())
}

#[derive(FromRow)]
struct WorkspaceRow {
    id: Uuid,
    public_id: String,
    name: String,
    description: Option<String>,
    status: String,
}
impl From<WorkspaceRow> for Workspace {
    fn from(row: WorkspaceRow) -> Self {
        Self {
            id: row.id,
            public_id: row.public_id,
            name: row.name,
            description: row.description,
            status: row.status,
        }
    }
}
#[derive(FromRow)]
struct MemberRow {
    id: Uuid,
    public_id: String,
    network_slug: String,
    address: String,
    client_ref: Option<String>,
    labels: Vec<String>,
}
#[derive(FromRow)]
struct ActivityRow {
    public_id: String,
    event_type: String,
    actor_kind: String,
    payload_version: i32,
    payload: Value,
    occurred_at: String,
}
impl From<ActivityRow> for WorkspaceActivityEvent {
    fn from(row: ActivityRow) -> Self {
        Self {
            public_id: row.public_id,
            event_type: row.event_type,
            actor_kind: row.actor_kind,
            payload_version: row.payload_version,
            payload: row.payload,
            occurred_at: row.occurred_at,
        }
    }
}
impl From<MemberRow> for WorkspaceMemberAddress {
    fn from(row: MemberRow) -> Self {
        Self {
            id: row.id,
            public_id: row.public_id,
            network_slug: row.network_slug,
            address: row.address,
            client_ref: row.client_ref,
            labels: row.labels,
        }
    }
}
