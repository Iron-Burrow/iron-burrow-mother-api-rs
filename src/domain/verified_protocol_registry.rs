use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::domain::{
    canonical_registry::CanonicalRegistry,
    defi::{RealizedYieldProtocol, RealizedYieldReserve},
    validation::{is_asset_slug, is_evm_address},
};

const EMBEDDED_PROTOCOLS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/reference-data/verified-protocols.json"
));
const PROTOCOLS_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub(crate) enum VerifiedProtocolRegistryError {
    #[error("failed to parse verified protocol declarations: {0}")]
    Parse(serde_json::Error),
    #[error("invalid verified protocol declarations: {0}")]
    Invalid(String),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProtocolCatalog {
    version: u32,
    protocols: Vec<ProtocolDeclaration>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProtocolDeclaration {
    slug: String,
    network_slug: String,
    family: String,
    adapter_kind: String,
    adapter_version: String,
    enabled: bool,
    verified: bool,
    targets: Vec<ProtocolTargetDeclaration>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProtocolTargetDeclaration {
    key: String,
    kind: String,
    asset_slug: Option<String>,
    address: String,
    enabled: bool,
    verified: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct VerifiedProtocolRegistry {
    protocols_by_slug: HashMap<String, RealizedYieldProtocol>,
}

impl VerifiedProtocolRegistry {
    pub(crate) fn from_embedded(
        canonical: &CanonicalRegistry,
    ) -> Result<Self, VerifiedProtocolRegistryError> {
        Self::from_json(EMBEDDED_PROTOCOLS_JSON, canonical)
    }

    pub(crate) fn from_json(
        json: &str,
        canonical: &CanonicalRegistry,
    ) -> Result<Self, VerifiedProtocolRegistryError> {
        let catalog = serde_json::from_str::<ProtocolCatalog>(json)
            .map_err(VerifiedProtocolRegistryError::Parse)?;
        if catalog.version != PROTOCOLS_VERSION {
            return invalid(format!(
                "unsupported protocol catalog version {}, expected {PROTOCOLS_VERSION}",
                catalog.version
            ));
        }

        let mut protocols_by_slug = HashMap::new();
        for declaration in catalog.protocols {
            let protocol = materialize_protocol(declaration, canonical)?;
            if protocols_by_slug
                .insert(protocol.slug.clone(), protocol)
                .is_some()
            {
                return invalid("duplicate protocol slug".to_string());
            }
        }
        Ok(Self { protocols_by_slug })
    }

    pub(crate) fn realized_yield_protocol(&self, slug: &str) -> Option<&RealizedYieldProtocol> {
        self.protocols_by_slug.get(&slug.to_ascii_lowercase())
    }
}

fn materialize_protocol(
    declaration: ProtocolDeclaration,
    canonical: &CanonicalRegistry,
) -> Result<RealizedYieldProtocol, VerifiedProtocolRegistryError> {
    validate_slug("protocol slug", &declaration.slug)?;
    validate_slug("protocol network_slug", &declaration.network_slug)?;
    if declaration.family.trim().is_empty() {
        return invalid(format!(
            "protocol {:?} requires a non-empty family",
            declaration.slug
        ));
    }
    if !declaration.enabled || !declaration.verified {
        return invalid(format!(
            "protocol {:?} must be enabled and verified",
            declaration.slug
        ));
    }
    if !compiled_adapter_supported(&declaration.adapter_kind, &declaration.adapter_version) {
        return invalid(format!(
            "protocol {:?} declares unsupported adapter {:?} version {:?}",
            declaration.slug, declaration.adapter_kind, declaration.adapter_version
        ));
    }
    let network = canonical
        .network_by_slug(&declaration.network_slug)
        .filter(|network| network.status == "active")
        .ok_or_else(|| {
            VerifiedProtocolRegistryError::Invalid(format!(
                "protocol {:?} references an unknown or inactive network {:?}",
                declaration.slug, declaration.network_slug
            ))
        })?;
    let chain_id = network.chain_id.ok_or_else(|| {
        VerifiedProtocolRegistryError::Invalid(format!(
            "protocol {:?} requires an EIP-155 network",
            declaration.slug
        ))
    })?;

    let mut target_keys = HashSet::new();
    let mut pool_address = None;
    let mut reserves = Vec::new();
    for target in declaration.targets {
        if target.key.trim().is_empty() || !target_keys.insert(target.key.clone()) {
            return invalid(format!(
                "protocol {:?} has duplicate or blank target key",
                declaration.slug
            ));
        }
        if !target.enabled || !target.verified || !is_lowercase_evm_address(&target.address) {
            return invalid(format!(
                "protocol {:?} target {:?} is not a verified lowercase EVM address",
                declaration.slug, target.key
            ));
        }
        match target.kind.as_str() {
            "pool" => {
                if target.key != "pool"
                    || target.asset_slug.is_some()
                    || pool_address.replace(target.address).is_some()
                {
                    return invalid(format!(
                        "protocol {:?} must have exactly one pool target",
                        declaration.slug
                    ));
                }
            }
            "reserve" => {
                let asset_slug = target.asset_slug.ok_or_else(|| {
                    VerifiedProtocolRegistryError::Invalid(format!(
                        "protocol {:?} reserve {:?} has no asset slug",
                        declaration.slug, target.key
                    ))
                })?;
                validate_slug("reserve asset_slug", &asset_slug)?;
                if target.key != asset_slug {
                    return invalid(format!(
                        "protocol {:?} reserve key must equal asset slug",
                        declaration.slug
                    ));
                }
                let asset = canonical
                    .asset_by_slug(&asset_slug)
                    .filter(|asset| asset.status == "active")
                    .ok_or_else(|| {
                        VerifiedProtocolRegistryError::Invalid(format!(
                            "protocol {:?} reserve references unknown or inactive asset {:?}",
                            declaration.slug, asset_slug
                        ))
                    })?;
                let mapping = canonical
                    .mapping(&asset_slug, &declaration.network_slug)
                    .filter(|mapping| mapping.status == "active")
                    .ok_or_else(|| {
                        VerifiedProtocolRegistryError::Invalid(format!(
                            "protocol {:?} reserve {:?} has no active canonical mapping",
                            declaration.slug, asset_slug
                        ))
                    })?;
                if mapping.deployment_address.as_deref() != Some(target.address.as_str()) {
                    return invalid(format!(
                        "protocol {:?} reserve {:?} does not match its canonical mapping address",
                        declaration.slug, asset_slug
                    ));
                }
                reserves.push(RealizedYieldReserve {
                    asset_slug,
                    asset_symbol: asset.symbol.clone(),
                    underlying_asset_address: target.address,
                });
            }
            _ => {
                return invalid(format!(
                    "protocol {:?} has unsupported target kind {:?}",
                    declaration.slug, target.kind
                ))
            }
        }
    }
    let pool_address = pool_address.ok_or_else(|| {
        VerifiedProtocolRegistryError::Invalid(format!(
            "protocol {:?} has no pool target",
            declaration.slug
        ))
    })?;
    reserves.sort_by(|left, right| left.asset_slug.cmp(&right.asset_slug));
    if reserves.is_empty() {
        return invalid(format!(
            "protocol {:?} has no reserve targets",
            declaration.slug
        ));
    }
    Ok(RealizedYieldProtocol {
        slug: declaration.slug,
        network_slug: declaration.network_slug,
        chain_id,
        adapter_kind: declaration.adapter_kind,
        adapter_version: declaration.adapter_version,
        pool_address,
        reserves,
    })
}

fn compiled_adapter_supported(kind: &str, version: &str) -> bool {
    matches!((kind, version), ("aave_v3_realized_yield", "v1"))
}

fn validate_slug(label: &str, value: &str) -> Result<(), VerifiedProtocolRegistryError> {
    if is_asset_slug(value) {
        Ok(())
    } else {
        invalid(format!("{label} {:?} is not normalized", value))
    }
}

fn is_lowercase_evm_address(value: &str) -> bool {
    is_evm_address(value) && value == value.to_ascii_lowercase()
}

fn invalid<T>(message: String) -> Result<T, VerifiedProtocolRegistryError> {
    Err(VerifiedProtocolRegistryError::Invalid(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::canonical_registry::CanonicalRegistry;

    #[test]
    fn embedded_registry_projects_the_exact_aave_configuration() {
        let canonical = CanonicalRegistry::from_embedded_catalog().unwrap();
        let registry = VerifiedProtocolRegistry::from_embedded(&canonical).unwrap();
        let protocol = registry.realized_yield_protocol("aave-v3").unwrap();
        assert_eq!(protocol.network_slug, "eth-mainnet");
        assert_eq!(protocol.chain_id, 1);
        assert_eq!(
            protocol.pool_address,
            "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2"
        );
        assert_eq!(
            protocol
                .reserves
                .iter()
                .map(|reserve| reserve.asset_slug.as_str())
                .collect::<Vec<_>>(),
            vec!["dai", "gho", "usdc", "usdt"]
        );
    }

    #[test]
    fn malformed_relations_fail_before_runtime() {
        let canonical = CanonicalRegistry::from_embedded_catalog().unwrap();
        let mut value: serde_json::Value = serde_json::from_str(EMBEDDED_PROTOCOLS_JSON).unwrap();
        value["protocols"][0]["adapter_version"] = serde_json::json!("v2");
        assert!(VerifiedProtocolRegistry::from_json(&value.to_string(), &canonical).is_err());
        value = serde_json::from_str(EMBEDDED_PROTOCOLS_JSON).unwrap();
        value["protocols"][0]["targets"][1]["address"] =
            serde_json::json!("0x0000000000000000000000000000000000000000");
        assert!(VerifiedProtocolRegistry::from_json(&value.to_string(), &canonical).is_err());
    }

    #[test]
    fn blank_protocol_family_has_a_specific_diagnostic() {
        let canonical = CanonicalRegistry::from_embedded_catalog().unwrap();
        let mut value: serde_json::Value = serde_json::from_str(EMBEDDED_PROTOCOLS_JSON).unwrap();
        value["protocols"][0]["family"] = serde_json::json!("   ");

        let error = VerifiedProtocolRegistry::from_json(&value.to_string(), &canonical)
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "invalid verified protocol declarations: protocol \"aave-v3\" requires a non-empty family"
        );
    }

    #[test]
    fn disabled_protocol_retains_enabled_and_verified_diagnostic() {
        let canonical = CanonicalRegistry::from_embedded_catalog().unwrap();
        let mut value: serde_json::Value = serde_json::from_str(EMBEDDED_PROTOCOLS_JSON).unwrap();
        value["protocols"][0]["enabled"] = serde_json::json!(false);

        let error = VerifiedProtocolRegistry::from_json(&value.to_string(), &canonical)
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "invalid verified protocol declarations: protocol \"aave-v3\" must be enabled and verified"
        );
    }

    #[test]
    fn production_protocol_resolution_has_no_historical_postgres_path() {
        for source in [
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/state.rs")),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/application/defi_realized_yield.rs"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/application/portfolio_simulation.rs"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/adapters/http/data_lab.rs"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/adapters/postgres/api_keys.rs"
            )),
        ] {
            for forbidden in [
                "mother_api.capability",
                "mother_api.defi_protocol",
                "mother_api.defi_protocol_target",
                "DefiProtocolRepository",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "found {forbidden} in production source"
                );
            }
        }
    }
}
