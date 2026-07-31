use uuid::Uuid;

use crate::{
    adapters::postgres::{
        workspaces::{AddMemberOutcome, Workspace, WorkspaceActivityEvent, WorkspaceMemberAddress},
        WorkspaceRepository,
    },
    domain::validation::is_evm_address,
};

pub(crate) const MAX_WORKSPACE_ADDRESSES: usize = 100;
pub(crate) const MAX_ADDRESS_LABELS: usize = 20;

#[derive(Clone, Debug)]
pub(crate) struct WorkspaceService {
    repository: WorkspaceRepository,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum WorkspaceInputError {
    #[error("invalid workspace name")]
    InvalidName,
    #[error("invalid workspace description")]
    InvalidDescription,
    #[error("invalid member address")]
    InvalidAddress,
    #[error("unsupported workspace network")]
    UnsupportedNetwork,
    #[error("invalid client reference")]
    InvalidClientRef,
    #[error("invalid label")]
    InvalidLabel,
    #[error("workspace address limit reached")]
    AddressLimit,
    #[error("workspace address label limit reached")]
    LabelLimit,
}

impl WorkspaceService {
    pub(crate) fn new(repository: WorkspaceRepository) -> Self {
        Self { repository }
    }
    pub(crate) async fn list(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<Workspace>, crate::adapters::postgres::errors::RepositoryError> {
        self.repository.list_active(account_id).await
    }
    pub(crate) async fn create(
        &self,
        account_id: Uuid,
        name: &str,
        description: Option<&str>,
    ) -> Result<Workspace, WorkspaceServiceError> {
        let name = normalize_required(name, 120).ok_or(WorkspaceInputError::InvalidName)?;
        let description =
            normalize_optional(description, 1000).ok_or(WorkspaceInputError::InvalidDescription)?;
        Ok(self
            .repository
            .create(account_id, &name, description.as_deref())
            .await?)
    }
    pub(crate) async fn find(
        &self,
        account_id: Uuid,
        workspace_id: &str,
    ) -> Result<Option<Workspace>, crate::adapters::postgres::errors::RepositoryError> {
        self.repository.find_owned(account_id, workspace_id).await
    }
    pub(crate) async fn members(
        &self,
        workspace_id: Uuid,
    ) -> Result<Vec<WorkspaceMemberAddress>, crate::adapters::postgres::errors::RepositoryError>
    {
        self.repository.members(workspace_id).await
    }
    pub(crate) async fn activity(
        &self,
        workspace_id: Uuid,
        before: Option<&str>,
        limit: i64,
    ) -> Result<Vec<WorkspaceActivityEvent>, crate::adapters::postgres::errors::RepositoryError>
    {
        self.repository
            .list_activity(workspace_id, before, limit)
            .await
    }
    pub(crate) async fn find_member(
        &self,
        account_id: Uuid,
        workspace_id: &str,
        member_id: &str,
    ) -> Result<
        Option<(Workspace, WorkspaceMemberAddress)>,
        crate::adapters::postgres::errors::RepositoryError,
    > {
        self.repository
            .find_owned_member(account_id, workspace_id, member_id)
            .await
    }
    pub(crate) async fn rename(
        &self,
        account_id: Uuid,
        workspace_id: &str,
        name: &str,
    ) -> Result<bool, WorkspaceServiceError> {
        let name = normalize_required(name, 120).ok_or(WorkspaceInputError::InvalidName)?;
        Ok(self
            .repository
            .rename(account_id, workspace_id, &name)
            .await?)
    }
    pub(crate) async fn archive(
        &self,
        account_id: Uuid,
        workspace_id: &str,
        archived: bool,
    ) -> Result<bool, crate::adapters::postgres::errors::RepositoryError> {
        self.repository
            .set_archived(account_id, workspace_id, archived)
            .await
    }
    pub(crate) async fn add_member(
        &self,
        workspace: &Workspace,
        network_slug: &str,
        address: &str,
        client_ref: Option<&str>,
    ) -> Result<bool, WorkspaceServiceError> {
        if !crate::adapters::postgres::workspaces::BALANCE_NETWORKS.contains(&network_slug) {
            return Err(WorkspaceInputError::UnsupportedNetwork.into());
        }
        let address = address.trim().to_ascii_lowercase();
        if !is_evm_address(&address) {
            return Err(WorkspaceInputError::InvalidAddress.into());
        }
        let client_ref =
            normalize_optional(client_ref, 120).ok_or(WorkspaceInputError::InvalidClientRef)?;
        match self
            .repository
            .add_member(
                workspace.id,
                network_slug,
                &address,
                client_ref.as_deref(),
                MAX_WORKSPACE_ADDRESSES,
            )
            .await?
        {
            AddMemberOutcome::Added => Ok(true),
            AddMemberOutcome::AlreadyPresent | AddMemberOutcome::Inactive => Ok(false),
            AddMemberOutcome::LimitReached => Err(WorkspaceInputError::AddressLimit.into()),
        }
    }
    pub(crate) async fn add_label(
        &self,
        workspace: &Workspace,
        member: &WorkspaceMemberAddress,
        label: &str,
    ) -> Result<(), WorkspaceServiceError> {
        if workspace.status != "active" {
            return Ok(());
        }
        let label = normalize_required(label, 64).ok_or(WorkspaceInputError::InvalidLabel)?;
        if !member
            .labels
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&label))
            && member.labels.len() >= MAX_ADDRESS_LABELS
        {
            return Err(WorkspaceInputError::LabelLimit.into());
        }
        Ok(self.repository.add_label(member.id, &label).await?)
    }
    pub(crate) async fn remove_label(
        &self,
        workspace: &Workspace,
        member: &WorkspaceMemberAddress,
        label: &str,
    ) -> Result<(), WorkspaceServiceError> {
        if workspace.status != "active" {
            return Ok(());
        }
        let label = normalize_required(label, 64).ok_or(WorkspaceInputError::InvalidLabel)?;
        Ok(self.repository.remove_label(member.id, &label).await?)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum WorkspaceServiceError {
    #[error(transparent)]
    Input(#[from] WorkspaceInputError),
    #[error(transparent)]
    Repository(#[from] crate::adapters::postgres::errors::RepositoryError),
}

fn normalize_required(value: &str, maximum: usize) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty() && trimmed.chars().count() <= maximum).then(|| trimmed.to_string())
}
fn normalize_optional(value: Option<&str>, maximum: usize) -> Option<Option<String>> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) if value.chars().count() <= maximum => Some(Some(value.to_string())),
        None => Some(None),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use uuid::Uuid;

    use crate::{adapters::postgres::WorkspaceRepository, test_utils::postgres::migrated_pool};

    use super::{
        Workspace, WorkspaceInputError, WorkspaceService, WorkspaceServiceError,
        MAX_WORKSPACE_ADDRESSES,
    };

    async fn create_workspace(pool: &PgPool) -> (Uuid, WorkspaceService, Workspace) {
        let account_id = Uuid::new_v4();
        sqlx::query("insert into mother_api.ib_account (id, public_id) values ($1, $2)")
            .bind(account_id)
            .bind(format!("iba_{}", account_id.simple()))
            .execute(pool)
            .await
            .unwrap();

        let service = WorkspaceService::new(WorkspaceRepository::database(pool.clone()));
        let workspace = service
            .create(account_id, "Workspace test", None)
            .await
            .unwrap();

        (account_id, service, workspace)
    }

    async fn seed_members(pool: &PgPool, workspace_id: Uuid, count: usize) {
        for index in 0..count {
            let member_id = Uuid::new_v4();
            sqlx::query("insert into mother_api.workspace_member_address (id, public_id, workspace_id, network_slug, address) values ($1, $2, $3, 'eth-mainnet', $4)")
                .bind(member_id)
                .bind(format!("wma_{}", member_id.simple()))
                .bind(workspace_id)
                .bind(address(index))
                .execute(pool)
                .await
                .unwrap();
        }
    }

    async fn remove_workspace(pool: &PgPool, account_id: Uuid, workspace_id: Uuid) {
        sqlx::query("delete from mother_api.workspace_member_address where workspace_id = $1")
            .bind(workspace_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("delete from mother_api.workspace where id = $1")
            .bind(workspace_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("delete from mother_api.ib_account where id = $1")
            .bind(account_id)
            .execute(pool)
            .await
            .unwrap();
    }

    fn address(index: usize) -> String {
        format!("0x{index:040x}")
    }

    #[tokio::test]
    async fn workspace_member_limit_rejects_new_addresses_but_allows_duplicates() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        let (account_id, service, workspace) = create_workspace(&pool).await;
        seed_members(&pool, workspace.id, MAX_WORKSPACE_ADDRESSES).await;

        assert!(!service
            .add_member(&workspace, "eth-mainnet", &address(0), None)
            .await
            .unwrap());
        assert!(matches!(
            service
                .add_member(
                    &workspace,
                    "eth-mainnet",
                    &address(MAX_WORKSPACE_ADDRESSES),
                    None,
                )
                .await,
            Err(WorkspaceServiceError::Input(
                WorkspaceInputError::AddressLimit
            ))
        ));

        remove_workspace(&pool, account_id, workspace.id).await;
    }

    #[tokio::test]
    async fn concurrent_workspace_member_additions_do_not_exceed_the_limit() {
        let Some(pool) = migrated_pool().await else {
            return;
        };
        let (account_id, service, workspace) = create_workspace(&pool).await;
        seed_members(&pool, workspace.id, MAX_WORKSPACE_ADDRESSES - 1).await;
        let first_address = address(MAX_WORKSPACE_ADDRESSES - 1);
        let second_address = address(MAX_WORKSPACE_ADDRESSES);

        let (first, second) = tokio::join!(
            service.add_member(&workspace, "eth-mainnet", &first_address, None,),
            service.add_member(&workspace, "eth-mainnet", &second_address, None,),
        );

        let added_count = [matches!(&first, Ok(true)), matches!(&second, Ok(true))]
            .into_iter()
            .filter(|added| *added)
            .count();
        assert_eq!(added_count, 1);
        assert!(
            matches!(
                &first,
                Err(WorkspaceServiceError::Input(
                    WorkspaceInputError::AddressLimit
                ))
            ) || matches!(
                &second,
                Err(WorkspaceServiceError::Input(
                    WorkspaceInputError::AddressLimit
                ))
            )
        );
        let member_count = sqlx::query_scalar::<_, i64>(
            "select count(*) from mother_api.workspace_member_address where workspace_id = $1",
        )
        .bind(workspace.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(member_count, MAX_WORKSPACE_ADDRESSES as i64);

        remove_workspace(&pool, account_id, workspace.id).await;
    }
}
