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
values
  (
    'bitcoin',
    'BTC',
    'Bitcoin',
    'crypto',
    'crypto',
    '/assets/bitcoin',
    array['btc', 'bitcoin', 'bit coin'],
    '{"demo_seed": true}'::jsonb,
    'active',
    10,
    now()
  ),
  (
    'ethereum',
    'ETH',
    'Ethereum',
    'crypto',
    'crypto',
    '/assets/ethereum',
    array['eth', 'ether', 'ethereum'],
    '{"demo_seed": true}'::jsonb,
    'active',
    20,
    now()
  ),
  (
    'usdc',
    'USDC',
    'USD Coin',
    'crypto',
    'crypto',
    '/assets/usdc',
    array[
      'usdc',
      'usd coin',
      'usdc coin',
      'usdc coin usd',
      'circle usd coin',
      'circle usdc',
      'dollar coin'
    ],
    '{"demo_seed": true}'::jsonb,
    'active',
    30,
    now()
  ),
  (
    'wrapped-bitcoin',
    'WBTC',
    'Wrapped Bitcoin',
    'crypto',
    'crypto',
    '/assets/wrapped-bitcoin',
    array['wbtc', 'wrapped bitcoin', 'wrapped btc'],
    '{"demo_seed": true}'::jsonb,
    'active',
    35,
    now()
  ),
  (
    'gold',
    'XAU',
    'Gold',
    'commodity',
    'commodity',
    '/assets/gold',
    array[
      'gold',
      'oro',
      'oro de ley',
      'xau',
      'precious metal',
      'metal precioso'
    ],
    '{"demo_seed": true}'::jsonb,
    'active',
    40,
    now()
  ),
  (
    'mantle',
    'MNT',
    'Mantle',
    'crypto',
    'crypto',
    '/assets/mantle',
    array['mnt', 'mantle'],
    '{"demo_seed": true}'::jsonb,
    'active',
    50,
    now()
  ),
  (
    'near',
    'NEAR',
    'NEAR Protocol',
    'crypto',
    'crypto',
    '/assets/near',
    array['near', 'near protocol'],
    '{"demo_seed": true}'::jsonb,
    'active',
    60,
    now()
  ),
  (
    'aave',
    'AAVE',
    'Aave Token',
    'crypto',
    'crypto',
    '/assets/aave',
    array['aave'],
    '{"demo_seed": true}'::jsonb,
    'active',
    70,
    now()
  ),
  (
    'ausd',
    'AUSD',
    'AUSD Agora Finance',
    'crypto',
    'stablecoin',
    '/assets/ausd',
    array['ausd'],
    '{"demo_seed": true}'::jsonb,
    'active',
    80,
    now()
  ),
  (
    'usds',
    'USDS',
    'Sky Dollar',
    'crypto',
    'stablecoin',
    '/assets/usds',
    array['usds', 'sky dollar'],
    '{"demo_seed": true}'::jsonb,
    'active',
    90,
    now()
  ),
  (
    'fbtc',
    'FBTC',
    'FBTC FunctionBTC',
    'crypto',
    'crypto',
    '/assets/fbtc',
    array['fbtc'],
    '{"demo_seed": true}'::jsonb,
    'inactive',
    100,
    now()
  ),
  (
    'gho',
    'GHO',
    'GHO Stablecoin',
    'crypto',
    'stablecoin',
    '/assets/gho',
    array['gho'],
    '{"demo_seed": true}'::jsonb,
    'active',
    110,
    now()
  ),
  (
    'mpdao',
    'MPDAO',
    'MPDAO Governance Token',
    'crypto',
    'crypto',
    '/assets/mpdao',
    array['mpdao'],
    '{"demo_seed": true}'::jsonb,
    'active',
    120,
    now()
  ),
  (
    'stnear',
    'STNEAR',
    'Staked NEAR',
    'crypto',
    'crypto',
    '/assets/stnear',
    array['stnear', 'staked near'],
    '{"demo_seed": true}'::jsonb,
    'active',
    130,
    now()
  ),
  (
    'usdt',
    'USDT',
    'Tether USD',
    'crypto',
    'stablecoin',
    '/assets/usdt',
    array['usdt', 'tether', 'tether usd'],
    '{"demo_seed": true}'::jsonb,
    'active',
    140,
    now()
  ),
  (
    'usdt0',
    'USDT0',
    'USDT0 Tether',
    'crypto',
    'stablecoin',
    '/assets/usdt0',
    array['usdt0', 'usdt zero'],
    '{"demo_seed": true}'::jsonb,
    'active',
    150,
    now()
  ),
  (
    'usde',
    'USDe',
    'Ethena USDe',
    'crypto',
    'stablecoin',
    '/assets/usde',
    array['usde'],
    '{"demo_seed": true}'::jsonb,
    'active',
    160,
    now()
  ),
  (
    'wrapped-ether',
    'WETH',
    'Wrapped Ether',
    'crypto',
    'crypto',
    '/assets/wrapped-ether',
    array['weth', 'wrapped ether', 'wrapped eth'],
    '{"demo_seed": true}'::jsonb,
    'active',
    170,
    now()
  ),
  (
    'cmeth',
    'cmETH',
    'cmETH',
    'crypto',
    'crypto',
    '/assets/cmeth',
    array['cmeth', 'cmeth token'],
    '{"demo_seed": true}'::jsonb,
    'active',
    180,
    now()
  ),
  (
    'meth',
    'mETH',
    'mETH',
    'crypto',
    'crypto',
    '/assets/meth',
    array['meth', 'meth token'],
    '{"demo_seed": true}'::jsonb,
    'active',
    190,
    now()
  ),
  (
    'susde',
    'sUSDe',
    'Staked Ethena USDe',
    'crypto',
    'stablecoin',
    '/assets/susde',
    array['susde', 'staked usde'],
    '{"demo_seed": true}'::jsonb,
    'active',
    200,
    now()
  )
on conflict ((lower(slug))) where status = 'active' do update
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

insert into mother_api.network (
  slug,
  name,
  family,
  chain_id,
  caip2,
  metadata,
  status,
  sort_order,
  updated_at
)
values
  (
    'bitcoin-mainnet',
    'Bitcoin Mainnet',
    'bitcoin',
    null,
    'bip122:000000000019d6689c085ae165831e93',
    '{"demo_seed": true}'::jsonb,
    'active',
    10,
    now()
  ),
  (
    'eth-mainnet',
    'Ethereum Mainnet',
    'evm',
    1,
    'eip155:1',
    '{"demo_seed": true}'::jsonb,
    'active',
    20,
    now()
  ),
  (
    'base',
    'Base',
    'evm',
    8453,
    'eip155:8453',
    '{"demo_seed": true}'::jsonb,
    'active',
    30,
    now()
  ),
  (
    'mantle',
    'Mantle',
    'evm',
    5000,
    'eip155:5000',
    '{"demo_seed": true}'::jsonb,
    'active',
    40,
    now()
  ),
  (
    'arbitrum-one',
    'Arbitrum One',
    'evm',
    42161,
    'eip155:42161',
    '{"demo_seed": true}'::jsonb,
    'active',
    50,
    now()
  ),
  (
    'near',
    'NEAR Mainnet',
    'near',
    null,
    'near:mainnet',
    '{"demo_seed": true}'::jsonb,
    'active',
    60,
    now()
  )
on conflict ((lower(slug))) where status = 'active' do update
set
  name = excluded.name,
  family = excluded.family,
  chain_id = excluded.chain_id,
  caip2 = excluded.caip2,
  metadata = mother_api.network.metadata || excluded.metadata,
  status = excluded.status,
  sort_order = excluded.sort_order,
  updated_at = now();

with mappings as (
  select *
  from (
    values
      ('bitcoin', 'bitcoin-mainnet', true, null::text, null::bigint, 8, null::text, '{"demo_seed": true}'::jsonb, 10),
      ('ethereum', 'eth-mainnet', true, null::text, null::bigint, 18, null::text, '{"demo_seed": true}'::jsonb, 20),
      ('ethereum', 'base', true, null::text, null::bigint, 18, null::text, '{"demo_seed": true}'::jsonb, 30),
      ('mantle', 'mantle', true, null::text, null::bigint, 18, null::text, '{"demo_seed": true}'::jsonb, 40)
  ) as mapping(asset_slug, network_slug, is_native, deployment_address, deployment_block, decimals, token_standard, metadata, sort_order)
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
  mappings.is_native,
  mappings.deployment_address,
  mappings.deployment_block,
  mappings.decimals,
  mappings.token_standard,
  mappings.metadata,
  'active',
  mappings.sort_order,
  now()
from mappings
join mother_api.global_asset asset
  on asset.slug = mappings.asset_slug
join mother_api.network network
  on network.slug = mappings.network_slug
on conflict (network_id) where status = 'active' and is_native = true do update
set
  asset_id = excluded.asset_id,
  deployment_address = excluded.deployment_address,
  deployment_block = excluded.deployment_block,
  decimals = excluded.decimals,
  token_standard = excluded.token_standard,
  metadata = mother_api.asset_chain_map.metadata || excluded.metadata,
  status = excluded.status,
  sort_order = excluded.sort_order,
  updated_at = now();

with mappings as (
  select *
  from (
    values
      (
        'usdc',
        'base',
        false,
        '0x833589fcd6edb6e08f4c7c32d4f71b54bda02913',
        null::bigint,
        6,
        'erc20',
        '{"demo_seed": true, "source": "circle"}'::jsonb,
        50
      ),
      (
        'wrapped-bitcoin',
        'base',
        false,
        '0x0555e30da8f98308edb960aa94c0db47230d2b9c',
        null::bigint,
        8,
        'erc20',
        '{"demo_seed": true, "source": "basescan"}'::jsonb,
        60
      )
  ) as mapping(asset_slug, network_slug, is_native, deployment_address, deployment_block, decimals, token_standard, metadata, sort_order)
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
  mappings.is_native,
  mappings.deployment_address,
  mappings.deployment_block,
  mappings.decimals,
  mappings.token_standard,
  mappings.metadata,
  'active',
  mappings.sort_order,
  now()
from mappings
join mother_api.global_asset asset
  on asset.slug = mappings.asset_slug
join mother_api.network network
  on network.slug = mappings.network_slug
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
