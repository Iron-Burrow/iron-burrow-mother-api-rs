//! Product authorization concepts. HTTP routes and persistence adapters map
//! into this module; neither is the source of authorization truth.

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum Capability {
    BalancesRead,
    Erc20TransfersRead,
    WorkspaceActivityRead,
    CatalogRead,
    PricesRead,
    ScanRead,
    LabRead,
    TreasuryRead,
    TreasurySnapshotWrite,
    ReportsRead,
    ReportsWrite,
    ReportsDeliveryWrite,
}

impl Capability {
    pub(crate) const ALL: [Self; 12] = [
        Self::BalancesRead,
        Self::Erc20TransfersRead,
        Self::WorkspaceActivityRead,
        Self::CatalogRead,
        Self::PricesRead,
        Self::ScanRead,
        Self::LabRead,
        Self::TreasuryRead,
        Self::TreasurySnapshotWrite,
        Self::ReportsRead,
        Self::ReportsWrite,
        Self::ReportsDeliveryWrite,
    ];
    pub(crate) const LEGACY_BASELINE: [Self; 2] = [Self::BalancesRead, Self::Erc20TransfersRead];
    /// New Phase 6 capabilities are deliberately opt-in for API keys. Browser
    /// sessions receive their account grants directly; no existing or newly
    /// issued account key is broadened by a catalog migration.
    pub(crate) const ACCOUNT_BASELINE: [Self; 3] = [
        Self::BalancesRead,
        Self::Erc20TransfersRead,
        Self::WorkspaceActivityRead,
    ];
    pub(crate) const DATALAB_BROWSER_BASELINE: [Self; 6] = [
        Self::CatalogRead,
        Self::PricesRead,
        Self::ScanRead,
        Self::LabRead,
        Self::TreasuryRead,
        Self::TreasurySnapshotWrite,
    ];

    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::BalancesRead => "balances.read",
            Self::Erc20TransfersRead => "transfers.read",
            Self::WorkspaceActivityRead => "workspace.activity.read",
            Self::CatalogRead => "catalog.read",
            Self::PricesRead => "prices.read",
            Self::ScanRead => "scan.read",
            Self::LabRead => "lab.read",
            Self::TreasuryRead => "treasury.read",
            Self::TreasurySnapshotWrite => "treasury.snapshot.write",
            Self::ReportsRead => "reports.read",
            Self::ReportsWrite => "reports.write",
            Self::ReportsDeliveryWrite => "reports.delivery.write",
        }
    }

    #[allow(dead_code)]
    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::BalancesRead => "Read supported latest and historical balance snapshots.",
            Self::Erc20TransfersRead => "Search bounded ERC-20 transfers.",
            Self::WorkspaceActivityRead => "Read account-owned Workspace activity and evidence.",
            Self::CatalogRead => "Read authenticated Data Lab asset and network catalog views.",
            Self::PricesRead => "Read authenticated Data Lab price views.",
            Self::ScanRead => "Read authenticated Workspace-member Scan views.",
            Self::LabRead => "Run authenticated curated Data Lab research.",
            Self::TreasuryRead => "Read account-owned Workspace treasury snapshots.",
            Self::TreasurySnapshotWrite => "Capture account-owned Workspace treasury snapshots.",
            Self::ReportsRead => "Read account-owned asynchronous reports.",
            Self::ReportsWrite => "Request account-owned asynchronous reports.",
            Self::ReportsDeliveryWrite => "Deliver Bigwig asynchronous report terminal results.",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|capability| capability.id() == value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NetworkScope {
    Any,
    Exact(String),
}

impl NetworkScope {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        if value == "*" {
            return Some(Self::Any);
        }

        if value.is_empty() {
            return None;
        }

        Some(Self::Exact(value.to_string()))
    }

    fn permits(&self, requested_network_slug: Option<&str>) -> bool {
        match (self, requested_network_slug) {
            (Self::Any, _) => true,
            // Middleware authenticates before a JSON body is consumed. Exact
            // scope is checked after request validation with the canonical
            // network slug.
            (Self::Exact(_), None) => true,
            (Self::Exact(granted), Some(requested)) => granted == requested,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GrantStatus {
    Active,
    Expired,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CapabilityGrant {
    pub(crate) capability: Capability,
    pub(crate) network_scope: NetworkScope,
    pub(crate) status: GrantStatus,
}

impl CapabilityGrant {
    pub(crate) fn active(capability: Capability, network_scope: NetworkScope) -> Self {
        Self {
            capability,
            network_scope,
            status: GrantStatus::Active,
        }
    }

    fn permits(&self, request: &AuthorizationRequest) -> bool {
        self.status == GrantStatus::Active
            && self.capability == request.capability
            && self.network_scope.permits(request.network_slug.as_deref())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AuthorizationRequest {
    pub(crate) capability: Capability,
    pub(crate) network_slug: Option<String>,
}

impl AuthorizationRequest {
    pub(crate) fn route(capability: Capability) -> Self {
        Self {
            capability,
            network_slug: None,
        }
    }

    pub(crate) fn network(capability: Capability, network_slug: &str) -> Self {
        Self {
            capability,
            network_slug: Some(network_slug.to_string()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AuthorizationContext {
    /// Compatibility owner grants. In the first slice this is the existing
    /// API consumer boundary; SPEC-014 will replace it with an IBAccount
    /// boundary without allowing keys to become broader.
    pub(crate) owner_grants: Vec<CapabilityGrant>,
    pub(crate) key_grants: Vec<CapabilityGrant>,
    /// Delegated Client grants apply only to agent keys. Their absence means
    /// the credential is owned directly by the compatibility owner/account.
    pub(crate) client_grants: Option<Vec<CapabilityGrant>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthorizationDeny {
    OwnerCapabilityNotGranted,
    ClientCapabilityNotGranted,
    KeyCapabilityNotGranted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthorizationDecision {
    Allow,
    Deny(AuthorizationDeny),
}

pub(crate) fn evaluate_authorization(
    context: &AuthorizationContext,
    request: &AuthorizationRequest,
) -> AuthorizationDecision {
    if !context
        .owner_grants
        .iter()
        .any(|grant| grant.permits(request))
    {
        return AuthorizationDecision::Deny(AuthorizationDeny::OwnerCapabilityNotGranted);
    }

    if let Some(grants) = context.client_grants.as_ref() {
        if !grants.iter().any(|grant| grant.permits(request)) {
            return AuthorizationDecision::Deny(AuthorizationDeny::ClientCapabilityNotGranted);
        }
    }

    if !context
        .key_grants
        .iter()
        .any(|grant| grant.permits(request))
    {
        return AuthorizationDecision::Deny(AuthorizationDeny::KeyCapabilityNotGranted);
    }

    AuthorizationDecision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(capability: Capability, scope: NetworkScope) -> CapabilityGrant {
        CapabilityGrant::active(capability, scope)
    }

    #[test]
    fn legacy_baseline_stays_pinned_to_the_compatibility_capabilities() {
        assert_eq!(Capability::LEGACY_BASELINE.len(), 2);
        assert_eq!(Capability::ACCOUNT_BASELINE.len(), 3);
    }

    #[test]
    fn authorization_is_the_intersection_of_owner_and_key_grants() {
        let cases = [
            (
                "owner deny overrides key allow",
                vec![],
                vec![grant(Capability::BalancesRead, NetworkScope::Any)],
                AuthorizationRequest::route(Capability::BalancesRead),
                AuthorizationDecision::Deny(AuthorizationDeny::OwnerCapabilityNotGranted),
            ),
            (
                "key narrows owner grant",
                vec![grant(Capability::BalancesRead, NetworkScope::Any)],
                vec![],
                AuthorizationRequest::route(Capability::BalancesRead),
                AuthorizationDecision::Deny(AuthorizationDeny::KeyCapabilityNotGranted),
            ),
            (
                "matching grants allow",
                vec![grant(Capability::BalancesRead, NetworkScope::Any)],
                vec![grant(Capability::BalancesRead, NetworkScope::Any)],
                AuthorizationRequest::route(Capability::BalancesRead),
                AuthorizationDecision::Allow,
            ),
            (
                "network scope is enforced on both boundaries",
                vec![grant(
                    Capability::BalancesRead,
                    NetworkScope::Exact("eth-mainnet".to_string()),
                )],
                vec![grant(
                    Capability::BalancesRead,
                    NetworkScope::Exact("eth-mainnet".to_string()),
                )],
                AuthorizationRequest::network(Capability::BalancesRead, "base-mainnet"),
                AuthorizationDecision::Deny(AuthorizationDeny::OwnerCapabilityNotGranted),
            ),
            (
                "expired grants do not allow",
                vec![CapabilityGrant {
                    capability: Capability::BalancesRead,
                    network_scope: NetworkScope::Any,
                    status: GrantStatus::Expired,
                }],
                vec![grant(Capability::BalancesRead, NetworkScope::Any)],
                AuthorizationRequest::route(Capability::BalancesRead),
                AuthorizationDecision::Deny(AuthorizationDeny::OwnerCapabilityNotGranted),
            ),
        ];

        for (name, owner_grants, key_grants, request, expected) in cases {
            assert_eq!(
                evaluate_authorization(
                    &AuthorizationContext {
                        owner_grants,
                        key_grants,
                        client_grants: None,
                    },
                    &request,
                ),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn client_grants_apply_only_to_agent_keys_and_empty_sets_deny() {
        let request = AuthorizationRequest::route(Capability::BalancesRead);
        let owner_grants = vec![grant(Capability::BalancesRead, NetworkScope::Any)];
        let key_grants = vec![grant(Capability::BalancesRead, NetworkScope::Any)];

        assert_eq!(
            evaluate_authorization(
                &AuthorizationContext {
                    owner_grants: owner_grants.clone(),
                    key_grants: key_grants.clone(),
                    client_grants: None,
                },
                &request,
            ),
            AuthorizationDecision::Allow
        );

        assert_eq!(
            evaluate_authorization(
                &AuthorizationContext {
                    owner_grants: owner_grants.clone(),
                    key_grants: key_grants.clone(),
                    client_grants: Some(vec![]),
                },
                &request,
            ),
            AuthorizationDecision::Deny(AuthorizationDeny::ClientCapabilityNotGranted)
        );

        assert_eq!(
            evaluate_authorization(
                &AuthorizationContext {
                    owner_grants,
                    key_grants,
                    client_grants: Some(vec![grant(Capability::BalancesRead, NetworkScope::Any)]),
                },
                &request,
            ),
            AuthorizationDecision::Allow
        );
    }
}
