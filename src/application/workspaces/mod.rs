use uuid::Uuid;

use crate::{
    adapters::postgres::{
        workspaces::{Workspace, WorkspaceMemberAddress},
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
        if workspace.status != "active" {
            return Ok(false);
        }
        if !crate::adapters::postgres::workspaces::BALANCE_NETWORKS.contains(&network_slug) {
            return Err(WorkspaceInputError::UnsupportedNetwork.into());
        }
        let address = address.trim().to_ascii_lowercase();
        if !is_evm_address(&address) {
            return Err(WorkspaceInputError::InvalidAddress.into());
        }
        let client_ref =
            normalize_optional(client_ref, 120).ok_or(WorkspaceInputError::InvalidClientRef)?;
        if self.repository.members(workspace.id).await?.len() >= MAX_WORKSPACE_ADDRESSES {
            return Err(WorkspaceInputError::AddressLimit.into());
        }
        Ok(self
            .repository
            .add_member(workspace.id, network_slug, &address, client_ref.as_deref())
            .await?)
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
