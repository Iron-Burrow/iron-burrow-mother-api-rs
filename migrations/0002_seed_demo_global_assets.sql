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

-- Native Network Assets
with native_mappings as (
  select *
  from (
    values
      ('bitcoin', 'bitcoin-mainnet', 8, 10),
      ('ethereum', 'eth-mainnet', 18, 20),
      ('ethereum', 'arbitrum-one', 18, 30),
      ('ethereum', 'base', 18, 40),
      ('mantle', 'mantle', 18, 50),
      ('near', 'near', 24, 60)
  ) as mapping(asset_slug, network_slug, decimals, sort_order)
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
  true,
  null::text,
  null::bigint,
  native_mappings.decimals,
  'native',
  '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb,
  'active',
  native_mappings.sort_order,
  now()
from native_mappings
join mother_api.global_asset asset
  on asset.slug = native_mappings.asset_slug
join mother_api.network network
  on network.slug = native_mappings.network_slug
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

with deployed_mappings as (
  select *
  from (
    values
      -- Aave
      ('aave', 'arbitrum-one', '0xba5ddd1f9d7f570dc94a51479a000e3bce967196', 7410775::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 110),
      ('aave', 'base', '0x63706e401c06ac8513145b7687a14804d17f814b', 24522168::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 120),
      ('aave', 'eth-mainnet', '0x7fc66500c84a76ad7e9c93437bfc5ac33e2ddae9', 10926829::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 100),
      -- Meta Pool
      ('mpdao', 'eth-mainnet', '0x798bcb35d2d48c8ce7ef8171860b8d53a98b361d', 19585586::bigint, 6, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 220),
      ('mpdao', 'near', 'mpdao-token.near', null::bigint, 6, 'nep141', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 210),
      ('stnear', 'near', 'meta-pool.near', null::bigint, 24, 'nep141', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 230),
      -- Mantle
      ('mantle', 'eth-mainnet', '0x3c3a81e81dc49a522a592e7622a7e711c06bf354', 17519070::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 200),
      -- USDC and USDT
      ('usdc', 'arbitrum-one', '0xaf88d065e77c8cc2239327c5edb3a432268e5831', 34266938::bigint, 6, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 250),
      ('usdc', 'base', '0x833589fcd6edb6e08f4c7c32d4f71b54bda02913', 2797221::bigint, 6, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 260),
      ('usdc', 'eth-mainnet', '0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48', 6082465::bigint, 6, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 240),
      ('usdc', 'mantle', '0x09bc4e0d864854c6afb6eb9a9cdf58ac190d0df9', 5972::bigint, 6, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 280),
      ('usdc', 'near', '17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1', null::bigint, 6, 'nep141', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 270),
      ('usdt', 'eth-mainnet', '0xdac17f958d2ee523a2206206994597c13d831ec7', 4634748::bigint, 6, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 290),
      ('usdt', 'near', 'usdt.tether-token.near', null::bigint, 6, 'nep141', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 300),
      ('usdt0', 'arbitrum-one', '0xfd086bc7cd5c481dcc9c85ebe478a1c0b69fcbb9', 228105::bigint, 6, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 310),
      ('usdt0', 'mantle', '0x779ded0c9e1022225f8e0630b35a9b54be713736', 86937611::bigint, 6, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 320),
      -- ETH representations
      ('cmeth', 'eth-mainnet', '0xe6829d9a7ee3040e1276fa75293bde931859e8fa', 20439180::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 440),
      ('cmeth', 'mantle', '0xe6829d9a7ee3040e1276fa75293bde931859e8fa', 67226285::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 450),
      ('meth', 'mantle', '0xcda86a272531e8640cd7f1a92c01839911b90bb0', 22293073::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 460),
      -- Stablecoins
      ('ausd', 'eth-mainnet', '0x00000000efe302beaa2b3e6e1b18d08d69a9012a', 20257620::bigint, 6, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 130),
      ('ausd', 'mantle', '0x00000000efe302beaa2b3e6e1b18d08d69a9012a', 69361435::bigint, 6, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 140),
      ('gho', 'arbitrum-one', '0x7dff72693f6a4149b17e7c6314655f6a9f7c8b33', 224701178::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 190),
      ('gho', 'eth-mainnet', '0x40d16fc0246ad3160ccc09b8d0d3a2cd28ae6c2f', 17698470::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 180),
      ('susde', 'arbitrum-one', '0x211cc4dd073734da055fbf44a2b4667d5e5fe5d2', 189133410::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 490),
      ('susde', 'base', '0x211cc4dd073734da055fbf44a2b4667d5e5fe5d2', 15768618::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 480),
      ('susde', 'eth-mainnet', '0x9d39a5de30e57443bff2a8307a4256c8797a3497', 18571359::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 470),
      ('susde', 'mantle', '0x211cc4dd073734da055fbf44a2b4667d5e5fe5d2', 59995414::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 500),
      ('usde', 'arbitrum-one', '0x5d3a1ff2b6bab83b63cd9ad0787074081a52ef34', 189133001::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 350),
      ('usde', 'base', '0x5d3a1ff2b6bab83b63cd9ad0787074081a52ef34', 15768548::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 340),
      ('usde', 'eth-mainnet', '0x4c9edd5852cd905f086c759e8383e09bff1e68b3', 18571358::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 330),
      ('usde', 'mantle', '0x5d3a1ff2b6bab83b63cd9ad0787074081a52ef34', 59988676::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 360),
      ('usds', 'arbitrum-one', '0x6491c05a82219b8d1479057361ff1654749b876b', 298070730::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 170),
      ('usds', 'base', '0x820c137fa70c8691f0e44dc420a5e53c168921dc', 20884784::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 160),
      ('usds', 'eth-mainnet', '0xdc035d45d973e3ec169d2276ddab16f1e407384f', 20663730::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 150),
      -- Wrapped Tokens
      ('wrapped-bitcoin', 'arbitrum-one', '0x2f2a2543b76a4166549f7aab2e75bef0aefc5b0f', 2591::bigint, 8, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 390),
      ('wrapped-bitcoin', 'base', '0x0555e30da8f98308edb960aa94c0db47230d2b9c', 19979002::bigint, 8, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 380),
      ('wrapped-bitcoin', 'eth-mainnet', '0x2260fac5e5542a773aa44fbcfedf7c193bc2c599', 6766284::bigint, 8, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 370),
      ('wrapped-ether', 'arbitrum-one', '0x82af49447d8a07e3bd95bd0d56f35241523fbab1', 55::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 410),
      ('wrapped-ether', 'base', '0x4200000000000000000000000000000000000006', 0::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map", "deployment": "genesis"}'::jsonb, 420),
      ('wrapped-ether', 'eth-mainnet', '0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2', 4719568::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map"}'::jsonb, 400),
      ('wrapped-ether', 'mantle', '0xdeaddeaddeaddeaddeaddeaddeaddeaddead1111', 0::bigint, 18, 'erc20', '{"demo_seed": true, "source": "user_asset_chain_map", "deployment": "genesis"}'::jsonb, 430)
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
