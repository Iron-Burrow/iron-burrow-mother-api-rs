use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::{
    capabilities::Capability,
    validation::{is_asset_slug, is_evm_address},
};

const EMBEDDED_CATALOG_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/reference-data/catalog.json"
));
pub(crate) const CATALOG_VERSION: u32 = 2;

#[derive(Debug, thiserror::Error)]
pub(crate) enum CanonicalRegistryError {
    #[error("failed to parse canonical catalog: {0}")]
    Parse(serde_json::Error),
    #[error("invalid canonical catalog: {0}")]
    Invalid(String),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Catalog {
    pub(crate) version: u32,
    pub(crate) capabilities: Vec<CapabilityDeclaration>,
    pub(crate) assets: Vec<CanonicalAsset>,
    pub(crate) networks: Vec<CanonicalNetwork>,
    pub(crate) asset_chain_maps: Vec<CanonicalAssetChainMap>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CapabilityDeclaration {
    pub(crate) id: String,
    pub(crate) description: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CanonicalAsset {
    pub(crate) slug: String,
    pub(crate) symbol: String,
    pub(crate) name: String,
    pub(crate) asset_kind: String,
    pub(crate) category: Option<String>,
    pub(crate) canonical_path: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) metadata: Value,
    pub(crate) status: String,
    pub(crate) sort_order: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CanonicalNetwork {
    pub(crate) slug: String,
    pub(crate) name: String,
    pub(crate) family: String,
    pub(crate) chain_id: Option<i64>,
    pub(crate) caip2: Option<String>,
    pub(crate) metadata: Value,
    pub(crate) status: String,
    pub(crate) sort_order: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CanonicalAssetChainMap {
    pub(crate) asset_slug: String,
    pub(crate) network_slug: String,
    pub(crate) is_native: bool,
    pub(crate) deployment_address: Option<String>,
    pub(crate) deployment_block: Option<i64>,
    pub(crate) decimals: Option<i32>,
    pub(crate) token_standard: String,
    pub(crate) metadata: Value,
    pub(crate) status: String,
    pub(crate) sort_order: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CanonicalMatchConfidence {
    Slug,
    Symbol,
    Name,
    Alias,
}

impl CanonicalMatchConfidence {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Slug => "slug_exact",
            Self::Symbol => "symbol_exact",
            Self::Name => "name_exact",
            Self::Alias => "alias_exact",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Slug => 0,
            Self::Symbol => 1,
            Self::Name => 2,
            Self::Alias => 3,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CanonicalAssetMatch<'a> {
    pub(crate) asset: &'a CanonicalAsset,
    pub(crate) confidence: CanonicalMatchConfidence,
}

#[derive(Debug)]
pub(crate) struct CanonicalAssetDetail<'a> {
    pub(crate) asset: &'a CanonicalAsset,
    pub(crate) mappings: Vec<&'a CanonicalAssetChainMap>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CanonicalBalanceTarget<'a> {
    pub(crate) ordinal: usize,
    pub(crate) requested_asset_slug: &'a str,
    pub(crate) network: Option<&'a CanonicalNetwork>,
    pub(crate) asset: Option<&'a CanonicalAsset>,
    pub(crate) mapping: Option<&'a CanonicalAssetChainMap>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CanonicalErc20Metadata<'a> {
    pub(crate) asset: &'a CanonicalAsset,
    pub(crate) network: &'a CanonicalNetwork,
    pub(crate) mapping: &'a CanonicalAssetChainMap,
}

#[derive(Debug)]
pub(crate) struct CanonicalRegistry {
    assets: Vec<CanonicalAsset>,
    networks: Vec<CanonicalNetwork>,
    mappings: Vec<CanonicalAssetChainMap>,
    asset_by_slug: HashMap<String, usize>,
    asset_by_alias: HashMap<String, usize>,
    network_by_slug: HashMap<String, usize>,
    mapping_by_identity: HashMap<(String, String), usize>,
    erc20_by_network_address: HashMap<(String, String), usize>,
    active_asset_indexes: Vec<usize>,
    active_mapping_indexes_by_asset: HashMap<String, Vec<usize>>,
}

impl CanonicalRegistry {
    pub(crate) fn from_embedded_catalog() -> Result<Self, CanonicalRegistryError> {
        Self::from_json(EMBEDDED_CATALOG_JSON)
    }

    pub(crate) fn from_json(json: &str) -> Result<Self, CanonicalRegistryError> {
        let catalog = parse_catalog_json(json)?;
        Self::from_catalog(catalog)
    }

    pub(crate) fn from_catalog(catalog: Catalog) -> Result<Self, CanonicalRegistryError> {
        validate_catalog(&catalog)?;

        let mut asset_by_slug = HashMap::new();
        let mut asset_by_alias = HashMap::new();
        for (index, asset) in catalog.assets.iter().enumerate() {
            asset_by_slug.insert(asset.slug.clone(), index);
            for alias in &asset.aliases {
                asset_by_alias.insert(alias.clone(), index);
            }
        }

        let network_by_slug = catalog
            .networks
            .iter()
            .enumerate()
            .map(|(index, network)| (network.slug.clone(), index))
            .collect::<HashMap<_, _>>();
        let mapping_by_identity = catalog
            .asset_chain_maps
            .iter()
            .enumerate()
            .map(|(index, mapping)| {
                (
                    (mapping.asset_slug.clone(), mapping.network_slug.clone()),
                    index,
                )
            })
            .collect::<HashMap<_, _>>();

        let mut active_asset_indexes = catalog
            .assets
            .iter()
            .enumerate()
            .filter_map(|(index, asset)| is_active(&asset.status).then_some(index))
            .collect::<Vec<_>>();
        active_asset_indexes
            .sort_by(|left, right| asset_order(&catalog.assets[*left], &catalog.assets[*right]));

        let mut active_mapping_indexes_by_asset = HashMap::<String, Vec<usize>>::new();
        let mut erc20_by_network_address = HashMap::new();
        for (index, mapping) in catalog.asset_chain_maps.iter().enumerate() {
            let asset_index = asset_by_slug[&mapping.asset_slug];
            let network_index = network_by_slug[&mapping.network_slug];
            let asset = &catalog.assets[asset_index];
            let network = &catalog.networks[network_index];
            if !is_active(&mapping.status)
                || !is_active(&asset.status)
                || !is_active(&network.status)
            {
                continue;
            }

            active_mapping_indexes_by_asset
                .entry(mapping.asset_slug.clone())
                .or_default()
                .push(index);

            if mapping.token_standard == "erc20" {
                let address = mapping
                    .deployment_address
                    .as_ref()
                    .expect("validated ERC-20 mappings have a deployment address");
                erc20_by_network_address
                    .insert((mapping.network_slug.clone(), address.clone()), index);
            }
        }
        for indexes in active_mapping_indexes_by_asset.values_mut() {
            indexes.sort_by(|left, right| {
                mapping_order(
                    &catalog.asset_chain_maps[*left],
                    &catalog.asset_chain_maps[*right],
                )
            });
        }

        Ok(Self {
            assets: catalog.assets,
            networks: catalog.networks,
            mappings: catalog.asset_chain_maps,
            asset_by_slug,
            asset_by_alias,
            network_by_slug,
            mapping_by_identity,
            erc20_by_network_address,
            active_asset_indexes,
            active_mapping_indexes_by_asset,
        })
    }

    pub(crate) fn asset_by_slug(&self, slug: &str) -> Option<&CanonicalAsset> {
        self.asset_by_slug
            .get(&slug.to_ascii_lowercase())
            .map(|index| &self.assets[*index])
    }

    pub(crate) fn network_by_slug(&self, network_slug: &str) -> Option<&CanonicalNetwork> {
        self.network_by_slug
            .get(&network_slug.to_ascii_lowercase())
            .map(|index| &self.networks[*index])
    }

    pub(crate) fn mapping(
        &self,
        asset_slug: &str,
        network_slug: &str,
    ) -> Option<&CanonicalAssetChainMap> {
        self.mapping_by_identity
            .get(&(
                asset_slug.to_ascii_lowercase(),
                network_slug.to_ascii_lowercase(),
            ))
            .map(|index| &self.mappings[*index])
    }

    pub(crate) fn find_confident_asset(
        &self,
        normalized_query: &str,
    ) -> Option<CanonicalAssetMatch<'_>> {
        let normalized_query = normalized_query.to_ascii_lowercase();
        let mut candidates = self
            .active_asset_indexes
            .iter()
            .filter_map(|index| {
                let asset = &self.assets[*index];
                let confidence = if asset.slug.eq_ignore_ascii_case(&normalized_query) {
                    CanonicalMatchConfidence::Slug
                } else if asset.symbol.eq_ignore_ascii_case(&normalized_query) {
                    CanonicalMatchConfidence::Symbol
                } else if asset.name.eq_ignore_ascii_case(&normalized_query) {
                    CanonicalMatchConfidence::Name
                } else if self
                    .asset_by_alias
                    .get(&normalized_query)
                    .is_some_and(|alias_index| alias_index == index)
                {
                    CanonicalMatchConfidence::Alias
                } else {
                    return None;
                };
                Some(CanonicalAssetMatch { asset, confidence })
            })
            .collect::<Vec<_>>();

        candidates.sort_by(|left, right| {
            left.confidence
                .rank()
                .cmp(&right.confidence.rank())
                .then_with(|| asset_order(left.asset, right.asset))
        });
        candidates.into_iter().next()
    }

    pub(crate) fn recommendations(
        &self,
        normalized_query: &str,
        limit: usize,
    ) -> Vec<&CanonicalAsset> {
        let query = normalized_query.to_ascii_lowercase();
        let matches = self
            .active_asset_indexes
            .iter()
            .map(|index| &self.assets[*index])
            .filter(|asset| asset_contains(asset, &query))
            .take(limit)
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return self.active_assets(limit);
        }
        matches
    }

    pub(crate) fn active_assets(&self, limit: usize) -> Vec<&CanonicalAsset> {
        self.active_asset_indexes
            .iter()
            .take(limit)
            .map(|index| &self.assets[*index])
            .collect()
    }

    pub(crate) fn asset_detail(&self, slug: &str) -> Option<CanonicalAssetDetail<'_>> {
        let asset_index = *self.asset_by_slug.get(&slug.to_ascii_lowercase())?;
        let asset = &self.assets[asset_index];
        if !is_active(&asset.status) {
            return None;
        }
        let mappings = self
            .active_mapping_indexes_by_asset
            .get(&asset.slug)
            .into_iter()
            .flatten()
            .map(|index| &self.mappings[*index])
            .collect();
        Some(CanonicalAssetDetail { asset, mappings })
    }

    pub(crate) fn ordered_balance_targets<'a>(
        &'a self,
        network_slug: &str,
        requested_asset_slugs: &'a [String],
    ) -> Vec<CanonicalBalanceTarget<'a>> {
        let network = self
            .network_by_slug(network_slug)
            .filter(|network| is_active(&network.status));
        requested_asset_slugs
            .iter()
            .enumerate()
            .map(|(index, requested_asset_slug)| {
                let asset = self
                    .asset_by_slug(requested_asset_slug)
                    .filter(|asset| is_active(&asset.status));
                let mapping = match (network, asset) {
                    (Some(network), Some(asset)) => self
                        .mapping(&asset.slug, &network.slug)
                        .filter(|mapping| is_active(&mapping.status)),
                    _ => None,
                };
                CanonicalBalanceTarget {
                    ordinal: index + 1,
                    requested_asset_slug,
                    network,
                    asset,
                    mapping,
                }
            })
            .collect()
    }

    pub(crate) fn erc20_metadata(
        &self,
        network_slug: &str,
        contract_address: &str,
    ) -> Option<CanonicalErc20Metadata<'_>> {
        let mapping_index = self.erc20_by_network_address.get(&(
            network_slug.to_ascii_lowercase(),
            contract_address.to_ascii_lowercase(),
        ))?;
        let mapping = &self.mappings[*mapping_index];
        let asset = self.asset_by_slug(&mapping.asset_slug)?;
        let network = self.network_by_slug(&mapping.network_slug)?;
        Some(CanonicalErc20Metadata {
            asset,
            network,
            mapping,
        })
    }
}

pub(crate) fn embedded_catalog_json() -> &'static str {
    EMBEDDED_CATALOG_JSON
}

pub(crate) fn parse_catalog_json(json: &str) -> Result<Catalog, CanonicalRegistryError> {
    let catalog = serde_json::from_str::<Catalog>(json).map_err(CanonicalRegistryError::Parse)?;
    validate_catalog(&catalog)?;
    Ok(catalog)
}

pub(crate) fn validate_catalog(catalog: &Catalog) -> Result<(), CanonicalRegistryError> {
    if catalog.version != CATALOG_VERSION {
        return invalid(format!(
            "unsupported catalog version {}, expected {CATALOG_VERSION}",
            catalog.version
        ));
    }

    validate_capabilities(&catalog.capabilities)?;

    let mut asset_slugs = HashSet::new();
    for asset in &catalog.assets {
        validate_asset(asset)?;
        if !asset_slugs.insert(asset.slug.as_str()) {
            return invalid(format!("duplicate asset slug {:?}", asset.slug));
        }
    }

    let mut aliases = HashMap::<&str, &str>::new();
    for asset in &catalog.assets {
        for alias in &asset.aliases {
            if alias != &asset.slug && asset_slugs.contains(alias.as_str()) {
                return invalid(format!(
                    "asset {:?} alias {:?} collides with asset slug",
                    asset.slug, alias
                ));
            }
            if let Some(previous_asset_slug) = aliases.insert(alias, &asset.slug) {
                if previous_asset_slug != asset.slug {
                    return invalid(format!(
                        "asset alias {:?} is shared by assets {:?} and {:?}",
                        alias, previous_asset_slug, asset.slug
                    ));
                }
            }
        }
    }

    let mut network_slugs = HashSet::new();
    let mut networks_by_slug = HashMap::new();
    for network in &catalog.networks {
        validate_network(network)?;
        if !network_slugs.insert(network.slug.as_str()) {
            return invalid(format!("duplicate network slug {:?}", network.slug));
        }
        networks_by_slug.insert(network.slug.as_str(), network);
    }

    let mut mapping_keys = HashSet::new();
    let mut active_native_networks = HashSet::new();
    let mut active_network_addresses = HashSet::new();
    let mut active_erc20_addresses = HashSet::new();

    for mapping in &catalog.asset_chain_maps {
        validate_slug("mapping asset_slug", &mapping.asset_slug)?;
        validate_slug("mapping network_slug", &mapping.network_slug)?;
        validate_status("mapping", &mapping.status)?;
        validate_sort_order("mapping", mapping.sort_order)?;
        validate_metadata("mapping", &mapping.asset_slug, &mapping.metadata)?;

        if !asset_slugs.contains(mapping.asset_slug.as_str()) {
            return invalid(format!(
                "asset_chain_map references undeclared asset {:?}",
                mapping.asset_slug
            ));
        }
        let network = networks_by_slug
            .get(mapping.network_slug.as_str())
            .ok_or_else(|| {
                CanonicalRegistryError::Invalid(format!(
                    "asset_chain_map references undeclared network {:?}",
                    mapping.network_slug
                ))
            })?;
        validate_mapping(mapping, network)?;

        let mapping_key = (mapping.asset_slug.as_str(), mapping.network_slug.as_str());
        if !mapping_keys.insert(mapping_key) {
            return invalid(format!(
                "duplicate asset_chain_map identity ({:?}, {:?})",
                mapping.asset_slug, mapping.network_slug
            ));
        }

        if is_active(&mapping.status)
            && mapping.is_native
            && !active_native_networks.insert(mapping.network_slug.as_str())
        {
            return invalid(format!(
                "duplicate active native mapping for network {:?}",
                mapping.network_slug
            ));
        }

        if is_active(&mapping.status) {
            if let Some(address) = mapping.deployment_address.as_deref() {
                let address_key = (mapping.network_slug.as_str(), address);
                if !active_network_addresses.insert(address_key) {
                    return invalid(format!(
                        "duplicate active deployment address {:?} on network {:?}",
                        address, mapping.network_slug
                    ));
                }
                if mapping.token_standard == "erc20" && !active_erc20_addresses.insert(address_key)
                {
                    return invalid(format!(
                        "duplicate active ERC-20 deployment address {:?} on network {:?}",
                        address, mapping.network_slug
                    ));
                }
            }
        }
    }

    Ok(())
}

fn validate_capabilities(
    capabilities: &[CapabilityDeclaration],
) -> Result<(), CanonicalRegistryError> {
    let mut declared_capabilities = HashSet::new();
    for capability in capabilities {
        validate_non_empty("capability id", &capability.id)?;
        validate_non_empty("capability description", &capability.description)?;
        if Capability::parse(&capability.id).is_none() {
            return invalid(format!("unknown capability id {:?}", capability.id));
        }
        if !declared_capabilities.insert(capability.id.as_str()) {
            return invalid(format!("duplicate capability id {:?}", capability.id));
        }
    }

    let required_capabilities = Capability::ALL
        .into_iter()
        .map(Capability::id)
        .collect::<HashSet<_>>();
    if declared_capabilities != required_capabilities {
        return invalid("capability declarations must match the application registry".to_string());
    }
    Ok(())
}

fn validate_asset(asset: &CanonicalAsset) -> Result<(), CanonicalRegistryError> {
    validate_slug("asset slug", &asset.slug)?;
    validate_non_empty("asset symbol", &asset.symbol)?;
    validate_non_empty("asset name", &asset.name)?;
    validate_non_empty("asset kind", &asset.asset_kind)?;
    if let Some(category) = asset.category.as_deref() {
        validate_non_empty("asset category", category)?;
    }
    if asset.canonical_path != format!("/assets/{}", asset.slug) {
        return invalid(format!(
            "asset {:?} has invalid canonical_path {:?}",
            asset.slug, asset.canonical_path
        ));
    }
    validate_status("asset", &asset.status)?;
    validate_sort_order("asset", asset.sort_order)?;
    validate_aliases(&asset.slug, &asset.aliases)?;
    validate_metadata("asset", &asset.slug, &asset.metadata)
}

fn validate_network(network: &CanonicalNetwork) -> Result<(), CanonicalRegistryError> {
    validate_slug("network slug", &network.slug)?;
    validate_non_empty("network name", &network.name)?;
    validate_non_empty("network family", &network.family)?;
    validate_status("network", &network.status)?;
    validate_sort_order("network", network.sort_order)?;
    validate_metadata("network", &network.slug, &network.metadata)?;
    if let Some(caip2) = network.caip2.as_deref() {
        let mut parts = caip2.split(':');
        let namespace = parts.next();
        let reference = parts.next();
        if namespace.is_none_or(str::is_empty)
            || reference.is_none_or(str::is_empty)
            || parts.next().is_some()
        {
            return invalid(format!(
                "network {:?} has invalid caip2 {:?}",
                network.slug, caip2
            ));
        }
    }

    if network.family == "evm" {
        let chain_id = network
            .chain_id
            .filter(|chain_id| *chain_id > 0)
            .ok_or_else(|| {
                CanonicalRegistryError::Invalid(format!(
                    "evm network {:?} requires positive chain_id",
                    network.slug
                ))
            })?;
        let expected_caip2 = format!("eip155:{chain_id}");
        if network.caip2.as_deref() != Some(expected_caip2.as_str()) {
            return invalid(format!(
                "evm network {:?} requires caip2 {:?}",
                network.slug, expected_caip2
            ));
        }
    } else if network.chain_id.is_some() {
        return invalid(format!(
            "non-evm network {:?} must not declare chain_id",
            network.slug
        ));
    }

    Ok(())
}

fn validate_mapping(
    mapping: &CanonicalAssetChainMap,
    network: &CanonicalNetwork,
) -> Result<(), CanonicalRegistryError> {
    let decimals = mapping.decimals.ok_or_else(|| {
        CanonicalRegistryError::Invalid(format!(
            "mapping ({:?}, {:?}) requires decimals",
            mapping.asset_slug, mapping.network_slug
        ))
    })?;
    if !(0..=255).contains(&decimals) {
        return invalid(format!(
            "mapping ({:?}, {:?}) has invalid decimals {}",
            mapping.asset_slug, mapping.network_slug, decimals
        ));
    }

    if mapping.is_native {
        if mapping.deployment_address.is_some() {
            return invalid(format!(
                "native mapping ({:?}, {:?}) must not declare deployment_address",
                mapping.asset_slug, mapping.network_slug
            ));
        }
        if mapping.deployment_block.is_some() {
            return invalid(format!(
                "native mapping ({:?}, {:?}) must not declare deployment_block",
                mapping.asset_slug, mapping.network_slug
            ));
        }
        if mapping.token_standard != "native" {
            return invalid(format!(
                "native mapping ({:?}, {:?}) requires token_standard \"native\"",
                mapping.asset_slug, mapping.network_slug
            ));
        }
        return Ok(());
    }

    validate_non_empty("mapping token_standard", &mapping.token_standard)?;
    if mapping.token_standard == "native" {
        return invalid(format!(
            "non-native mapping ({:?}, {:?}) must not use token_standard \"native\"",
            mapping.asset_slug, mapping.network_slug
        ));
    }
    let address = mapping.deployment_address.as_deref().ok_or_else(|| {
        CanonicalRegistryError::Invalid(format!(
            "non-native mapping ({:?}, {:?}) requires deployment_address",
            mapping.asset_slug, mapping.network_slug
        ))
    })?;
    validate_non_empty("mapping deployment_address", address)?;
    if address != address.to_ascii_lowercase() {
        return invalid(format!(
            "mapping ({:?}, {:?}) requires lowercase deployment_address",
            mapping.asset_slug, mapping.network_slug
        ));
    }
    if mapping.deployment_block.is_some_and(|block| block < 0) {
        return invalid(format!(
            "mapping ({:?}, {:?}) has negative deployment_block",
            mapping.asset_slug, mapping.network_slug
        ));
    }
    if mapping.token_standard == "erc20" {
        if network.family != "evm" {
            return invalid(format!(
                "erc20 mapping ({:?}, {:?}) requires an evm network",
                mapping.asset_slug, mapping.network_slug
            ));
        }
        if !is_evm_address(address) {
            return invalid(format!(
                "erc20 mapping ({:?}, {:?}) has invalid deployment_address {:?}",
                mapping.asset_slug, mapping.network_slug, address
            ));
        }
        if mapping.deployment_block.is_none() {
            return invalid(format!(
                "erc20 mapping ({:?}, {:?}) requires deployment_block",
                mapping.asset_slug, mapping.network_slug
            ));
        }
    }

    Ok(())
}

fn validate_slug(label: &str, slug: &str) -> Result<(), CanonicalRegistryError> {
    if is_asset_slug(slug) {
        Ok(())
    } else {
        invalid(format!("{label} {slug:?} is not normalized"))
    }
}

fn validate_non_empty(label: &str, value: &str) -> Result<(), CanonicalRegistryError> {
    if value.trim().is_empty() {
        invalid(format!("{label} must not be empty"))
    } else {
        Ok(())
    }
}

fn validate_aliases(asset_slug: &str, aliases: &[String]) -> Result<(), CanonicalRegistryError> {
    let mut seen = HashSet::new();
    for alias in aliases {
        if alias.is_empty() || alias.trim() != alias || alias != &alias.to_ascii_lowercase() {
            return invalid(format!(
                "asset {asset_slug:?} has non-normalized alias {alias:?}"
            ));
        }
        if !seen.insert(alias.as_str()) {
            return invalid(format!(
                "asset {asset_slug:?} has duplicate alias {alias:?}"
            ));
        }
    }
    Ok(())
}

fn validate_status(owner: &str, status: &str) -> Result<(), CanonicalRegistryError> {
    match status {
        "active" | "inactive" | "deprecated" | "hidden" | "pending" | "unsupported"
        | "archived" => Ok(()),
        _ => invalid(format!("{owner} has invalid status {status:?}")),
    }
}

fn validate_sort_order(owner: &str, sort_order: i32) -> Result<(), CanonicalRegistryError> {
    if sort_order < 0 {
        invalid(format!("{owner} has negative sort_order {sort_order}"))
    } else {
        Ok(())
    }
}

fn validate_metadata(
    owner: &str,
    identity: &str,
    metadata: &Value,
) -> Result<(), CanonicalRegistryError> {
    if metadata.is_object() {
        Ok(())
    } else {
        invalid(format!(
            "{owner} {identity:?} metadata must be a JSON object"
        ))
    }
}

fn invalid<T>(message: String) -> Result<T, CanonicalRegistryError> {
    Err(CanonicalRegistryError::Invalid(message))
}

fn is_active(status: &str) -> bool {
    status == "active"
}

fn asset_order(left: &CanonicalAsset, right: &CanonicalAsset) -> std::cmp::Ordering {
    left.sort_order.cmp(&right.sort_order).then_with(|| {
        left.symbol
            .to_ascii_lowercase()
            .cmp(&right.symbol.to_ascii_lowercase())
    })
}

fn mapping_order(
    left: &CanonicalAssetChainMap,
    right: &CanonicalAssetChainMap,
) -> std::cmp::Ordering {
    left.sort_order
        .cmp(&right.sort_order)
        .then_with(|| left.network_slug.cmp(&right.network_slug))
}

fn asset_contains(asset: &CanonicalAsset, normalized_query: &str) -> bool {
    asset.slug.to_ascii_lowercase().contains(normalized_query)
        || asset.symbol.to_ascii_lowercase().contains(normalized_query)
        || asset.name.to_ascii_lowercase().contains(normalized_query)
        || asset
            .aliases
            .iter()
            .any(|alias| alias.to_ascii_lowercase().contains(normalized_query))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn embedded_catalog_builds_without_a_database_pool() {
        let registry = CanonicalRegistry::from_embedded_catalog().unwrap();

        assert_eq!(registry.asset_by_slug("usdc").unwrap().symbol, "USDC");
        assert_eq!(
            registry.network_by_slug("ETH-MAINNET").unwrap().chain_id,
            Some(1)
        );
    }

    #[test]
    fn embedded_catalog_capabilities_match_the_runtime_registry() {
        let catalog = parse_catalog_json(embedded_catalog_json()).unwrap();
        let declared = catalog
            .capabilities
            .iter()
            .map(|capability| (capability.id.as_str(), capability.description.as_str()))
            .collect::<Vec<_>>();
        let runtime = Capability::ALL
            .into_iter()
            .map(|capability| (capability.id(), capability.description()))
            .collect::<Vec<_>>();

        assert_eq!(declared, runtime);
    }

    #[test]
    fn malformed_unknown_and_unsupported_catalogs_fail() {
        assert!(matches!(
            CanonicalRegistry::from_json("{").unwrap_err(),
            CanonicalRegistryError::Parse(_)
        ));
        let mut catalog = minimal_catalog("unknown-field");
        let mut value = serde_json::to_value(&catalog).unwrap();
        value["unknown"] = json!(true);
        assert!(matches!(
            CanonicalRegistry::from_json(&value.to_string()).unwrap_err(),
            CanonicalRegistryError::Parse(_)
        ));
        catalog.version = CATALOG_VERSION + 1;
        assert_invalid(catalog, "unsupported catalog version");
    }

    #[test]
    fn normalized_identity_and_alias_collisions_fail() {
        let mut catalog = minimal_catalog("duplicate-identity");
        catalog.assets.push(catalog.assets[0].clone());
        assert_invalid(catalog, "duplicate asset slug");

        let mut catalog = minimal_catalog("shared-alias");
        catalog.assets[0].aliases = vec!["shared alias".to_string()];
        let mut other = asset("other-shared-alias");
        other.aliases = vec!["shared alias".to_string()];
        catalog.assets.push(other);
        assert_invalid(catalog, "is shared by assets");

        let mut catalog = minimal_catalog("alias-slug");
        let mut other = asset("other-alias-slug");
        other.aliases = vec![catalog.assets[0].slug.clone()];
        catalog.assets.push(other);
        assert_invalid(catalog, "collides with asset slug");
    }

    #[test]
    fn mapping_identity_and_representation_invariants_fail() {
        let mut catalog = minimal_catalog("unknown-reference");
        catalog.asset_chain_maps[0].asset_slug = "missing".to_string();
        assert_invalid(catalog, "references undeclared asset");

        let mut catalog = minimal_catalog("duplicate-map");
        catalog
            .asset_chain_maps
            .push(catalog.asset_chain_maps[0].clone());
        assert_invalid(catalog, "duplicate asset_chain_map identity");

        let mut catalog = minimal_catalog("native-block");
        let mapping = &mut catalog.asset_chain_maps[0];
        mapping.is_native = true;
        mapping.deployment_address = None;
        mapping.token_standard = "native".to_string();
        assert_invalid(catalog, "must not declare deployment_block");

        let mut catalog = minimal_catalog("missing-block");
        catalog.asset_chain_maps[0].deployment_block = None;
        assert_invalid(catalog, "requires deployment_block");
    }

    #[test]
    fn erc20_and_network_invariants_fail() {
        let mut catalog = minimal_catalog("bad-address");
        catalog.asset_chain_maps[0].deployment_address = Some("0xnot-an-address".to_string());
        assert_invalid(catalog, "invalid deployment_address");

        let mut catalog = minimal_catalog("negative-decimals");
        catalog.asset_chain_maps[0].decimals = Some(-1);
        assert_invalid(catalog, "invalid decimals");

        let mut catalog = minimal_catalog("wrong-family");
        catalog.networks[0].family = "near".to_string();
        catalog.networks[0].chain_id = None;
        catalog.networks[0].caip2 = Some("near:mainnet".to_string());
        assert_invalid(catalog, "requires an evm network");

        let mut catalog = minimal_catalog("wrong-caip2");
        catalog.networks[0].caip2 = Some("eip155:2".to_string());
        assert_invalid(catalog, "requires caip2");
    }

    #[test]
    fn indexes_keep_inactive_records_out_of_active_views_and_order_results() {
        let mut catalog = minimal_catalog("ordering");
        let mut second = asset("second-ordering");
        second.symbol = "AAA".to_string();
        second.sort_order = 10;
        catalog.assets.push(second);
        let mut inactive = asset("inactive-ordering");
        inactive.status = "inactive".to_string();
        inactive.aliases = vec!["inactive".to_string()];
        catalog.assets.push(inactive);

        let registry = CanonicalRegistry::from_catalog(catalog).unwrap();
        assert_eq!(
            registry
                .active_assets(10)
                .into_iter()
                .map(|asset| asset.slug.as_str())
                .collect::<Vec<_>>(),
            vec!["second-ordering", "test-asset-ordering"]
        );
        assert!(registry.find_confident_asset("inactive").is_none());
        assert!(registry.asset_detail("inactive-ordering").is_none());
    }

    #[test]
    fn lookups_are_deterministic_and_normalize_erc20_addresses() {
        let registry = CanonicalRegistry::from_embedded_catalog().unwrap();
        let matched = registry.find_confident_asset("usdc").unwrap();
        assert_eq!(matched.asset.slug, "usdc");
        assert_eq!(matched.confidence.as_str(), "slug_exact");
        assert_eq!(
            registry
                .recommendations("not-present", 2)
                .into_iter()
                .map(|asset| asset.slug.as_str())
                .collect::<Vec<_>>(),
            vec!["bitcoin", "ethereum"]
        );
        let detail = registry.asset_detail("ethereum").unwrap();
        assert_eq!(detail.asset.slug, "ethereum");
        assert_eq!(
            detail
                .mappings
                .into_iter()
                .map(|mapping| mapping.network_slug.as_str())
                .collect::<Vec<_>>(),
            vec!["eth-mainnet", "arbitrum-mainnet", "base-mainnet"]
        );
        let requested_assets = ["usdc".to_string(), "missing".to_string()];
        let targets = registry.ordered_balance_targets("eth-mainnet", &requested_assets);
        assert_eq!(targets[0].ordinal, 1);
        assert_eq!(targets[0].requested_asset_slug, "usdc");
        assert_eq!(targets[0].network.unwrap().slug, "eth-mainnet");
        assert!(targets[0].mapping.is_some());
        assert!(targets[1].asset.is_none());
        let erc20 = registry
            .erc20_metadata("ETH-MAINNET", "0xA0B86991C6218B36C1D19D4A2E9EB0CE3606EB48")
            .unwrap();
        assert_eq!(erc20.asset.slug, "usdc");
        assert_eq!(erc20.network.slug, "eth-mainnet");
        assert_eq!(erc20.mapping.decimals, Some(6));
    }

    fn assert_invalid(catalog: Catalog, expected: &str) {
        let error = CanonicalRegistry::from_catalog(catalog).unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "expected error to contain {expected:?}, got {error}"
        );
    }

    fn minimal_catalog(suffix: &str) -> Catalog {
        let asset_slug = format!("test-asset-{suffix}");
        let network_slug = format!("test-network-{suffix}");
        let chain_id = 10_000;
        Catalog {
            version: CATALOG_VERSION,
            capabilities: Capability::ALL
                .into_iter()
                .map(|capability| CapabilityDeclaration {
                    id: capability.id().to_string(),
                    description: capability.description().to_string(),
                })
                .collect(),
            assets: vec![asset(&asset_slug)],
            networks: vec![CanonicalNetwork {
                slug: network_slug.clone(),
                name: "Test Network".to_string(),
                family: "evm".to_string(),
                chain_id: Some(chain_id),
                caip2: Some(format!("eip155:{chain_id}")),
                metadata: json!({}),
                status: "active".to_string(),
                sort_order: 10,
            }],
            asset_chain_maps: vec![CanonicalAssetChainMap {
                asset_slug,
                network_slug,
                is_native: false,
                deployment_address: Some("0x1111111111111111111111111111111111111111".to_string()),
                deployment_block: Some(1),
                decimals: Some(18),
                token_standard: "erc20".to_string(),
                metadata: json!({}),
                status: "active".to_string(),
                sort_order: 10,
            }],
        }
    }

    fn asset(slug: &str) -> CanonicalAsset {
        CanonicalAsset {
            slug: slug.to_string(),
            symbol: "TST".to_string(),
            name: format!("Test Asset {slug}"),
            asset_kind: "crypto".to_string(),
            category: Some("crypto".to_string()),
            canonical_path: format!("/assets/{slug}"),
            aliases: vec![slug.to_string()],
            metadata: json!({}),
            status: "active".to_string(),
            sort_order: 10,
        }
    }
}
