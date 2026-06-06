insert into mother_api.global_asset (
  slug,
  symbol,
  name,
  asset_kind,
  category,
  canonical_path,
  aliases,
  metadata,
  status,
  sort_order,
  updated_at
)
values (
  'bitso-mxn',
  'MXNB',
  'Bitso MXN',
  'crypto',
  'stablecoin',
  '/assets/bitso-mxn',
  array['mxnb', 'bitso mxn', 'bitso-mxn', 'mexican peso stablecoin'],
  '{"demo_seed": true}'::jsonb,
  'active',
  210,
  now()
)
on conflict (slug) do update
set
  symbol = excluded.symbol,
  name = excluded.name,
  asset_kind = excluded.asset_kind,
  category = excluded.category,
  canonical_path = excluded.canonical_path,
  aliases = excluded.aliases,
  metadata = mother_api.global_asset.metadata || excluded.metadata,
  status = excluded.status,
  sort_order = excluded.sort_order,
  updated_at = now();

with deployed_mappings as (
  select *
  from (
    values
      (
        'bitso-mxn',
        'arbitrum-one',
        '0xf197ffc28c23e0309b5559e7a166f2c6164c80aa',
        271756855::bigint,
        6,
        'erc20',
        '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb,
        520
      )
  ) as mapping(asset_slug, network_slug, deployment_address, deployment_block, decimals, token_standard, metadata, sort_order)
)
insert into mother_api.asset_chain_map (
  asset_id,
  network_id,
  is_native,
  deployment_address,
  deployment_block,
  decimals,
  token_standard,
  metadata,
  status,
  sort_order,
  updated_at
)
select
  asset.id,
  network.id,
  false,
  deployed_mappings.deployment_address,
  deployed_mappings.deployment_block,
  deployed_mappings.decimals,
  deployed_mappings.token_standard,
  deployed_mappings.metadata,
  'active',
  deployed_mappings.sort_order,
  now()
from deployed_mappings
join mother_api.global_asset asset
  on asset.slug = deployed_mappings.asset_slug
join mother_api.network network
  on network.slug = deployed_mappings.network_slug
on conflict (network_id, (lower(deployment_address)))
  where status = 'active' and deployment_address is not null do update
set
  asset_id = excluded.asset_id,
  is_native = excluded.is_native,
  deployment_block = excluded.deployment_block,
  decimals = excluded.decimals,
  token_standard = excluded.token_standard,
  metadata = mother_api.asset_chain_map.metadata || excluded.metadata,
  status = excluded.status,
  sort_order = excluded.sort_order,
  updated_at = now();
