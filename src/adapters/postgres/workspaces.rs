use sqlx::{FromRow, PgPool};
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
        sqlx::query_as::<_, WorkspaceRow>("insert into mother_api.workspace (id, public_id, owner_ib_account_id, name, description) values ($1, $2, $3, $4, $5) returning id, public_id, name, description, status")
            .bind(id).bind(public_id).bind(account_id).bind(name).bind(description).fetch_one(&self.0).await.map_err(RepositoryError::new).map(Into::into)
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
        sqlx::query("update mother_api.workspace set name = $3, updated_at = now() where owner_ib_account_id = $1 and public_id = $2 and status = 'active'")
            .bind(account_id).bind(public_id).bind(name).execute(&self.0).await.map_err(RepositoryError::new).map(|result| result.rows_affected() == 1)
    }

    pub(crate) async fn set_archived(
        &self,
        account_id: Uuid,
        public_id: &str,
        archived: bool,
    ) -> Result<bool, RepositoryError> {
        let target = if archived { "archived" } else { "active" };
        sqlx::query("update mother_api.workspace set status = $3, archived_at = case when $3 = 'archived' then now() else null end, updated_at = now() where owner_ib_account_id = $1 and public_id = $2 and status <> $3")
            .bind(account_id).bind(public_id).bind(target).execute(&self.0).await.map_err(RepositoryError::new).map(|result| result.rows_affected() == 1)
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
    ) -> Result<bool, RepositoryError> {
        let id = Uuid::new_v4();
        let public_id = format!("wma_{}", id.simple());
        let result = sqlx::query("insert into mother_api.workspace_member_address (id, public_id, workspace_id, network_slug, address, client_ref) values ($1, $2, $3, $4, $5, $6) on conflict (workspace_id, network_slug, address) do nothing")
            .bind(id).bind(public_id).bind(workspace_id).bind(network_slug).bind(address).bind(client_ref).execute(&self.0).await.map_err(RepositoryError::new)?;
        Ok(result.rows_affected() == 1)
    }

    pub(crate) async fn add_label(
        &self,
        member_id: Uuid,
        label: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query("insert into mother_api.workspace_member_address_label (member_address_id, label) values ($1, $2) on conflict do nothing")
            .bind(member_id).bind(label).execute(&self.0).await.map_err(RepositoryError::new).map(|_| ())
    }

    pub(crate) async fn remove_label(
        &self,
        member_id: Uuid,
        label: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query("delete from mother_api.workspace_member_address_label where member_address_id = $1 and lower(label) = lower($2)")
            .bind(member_id).bind(label).execute(&self.0).await.map_err(RepositoryError::new).map(|_| ())
    }
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
