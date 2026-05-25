create extension if not exists pgcrypto;

create schema if not exists mother_api;

comment on schema mother_api is
  'Mother API owned application schema. Contains Sentinel-facing global asset catalog data, not indexer-owned runtime tables.';

do $$
begin
  create type mother_api.global_asset_status as enum (
    'active',
    'inactive',
    'deprecated',
    'hidden',
    'pending',
    'unsupported',
    'archived'
  );
exception
  when duplicate_object then null;
end $$;

create table if not exists mother_api.global_asset (
  id uuid primary key default gen_random_uuid(),
  slug text not null,
  symbol text not null,
  name text not null,
  asset_kind text not null default 'crypto',
  category text,
  canonical_path text not null,
  aliases text[] not null default '{}',
  metadata jsonb not null default '{}'::jsonb,
  status mother_api.global_asset_status not null default 'active',
  sort_order integer not null default 1000,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table if not exists mother_api.network (
  id uuid primary key default gen_random_uuid(),
  slug text not null,
  name text not null,
  family text not null,
  chain_id bigint,
  caip2 text,
  metadata jsonb not null default '{}'::jsonb,
  status text not null default 'active',
  sort_order integer not null default 1000,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table if not exists mother_api.asset_chain_map (
  id uuid primary key default gen_random_uuid(),
  asset_id uuid not null references mother_api.global_asset (id) on delete cascade,
  network_id uuid not null references mother_api.network (id) on delete cascade,
  is_native boolean not null default false,
  deployment_address text,
  deployment_block bigint,
  decimals integer,
  token_standard text,
  metadata jsonb not null default '{}'::jsonb,
  status text not null default 'active',
  sort_order integer not null default 1000,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create unique index if not exists uq_mother_api_global_asset_active_slug_lower
  on mother_api.global_asset (lower(slug))
  where status = 'active';

create unique index if not exists uq_mother_api_global_asset_active_symbol_lower
  on mother_api.global_asset (lower(symbol))
  where status = 'active';

create index if not exists idx_mother_api_global_asset_status_sort
  on mother_api.global_asset (status, sort_order);

create index if not exists idx_mother_api_global_asset_slug_lower
  on mother_api.global_asset (lower(slug));

create index if not exists idx_mother_api_global_asset_symbol_lower
  on mother_api.global_asset (lower(symbol));

create index if not exists idx_mother_api_global_asset_name_lower
  on mother_api.global_asset (lower(name));

create index if not exists idx_mother_api_global_asset_aliases_gin
  on mother_api.global_asset using gin (aliases);

create unique index if not exists uq_mother_api_network_active_slug_lower
  on mother_api.network (lower(slug))
  where status = 'active';

create unique index if not exists uq_mother_api_network_caip2_lower
  on mother_api.network (lower(caip2))
  where caip2 is not null;

create index if not exists idx_mother_api_network_status_sort
  on mother_api.network (status, sort_order);

create unique index if not exists uq_mother_api_asset_chain_map_active_native_network
  on mother_api.asset_chain_map (network_id)
  where status = 'active' and is_native = true;

create unique index if not exists uq_mother_api_asset_chain_map_active_network_address
  on mother_api.asset_chain_map (network_id, lower(deployment_address))
  where status = 'active' and deployment_address is not null;

create index if not exists idx_mother_api_asset_chain_map_asset_status
  on mother_api.asset_chain_map (asset_id, status);

create index if not exists idx_mother_api_asset_chain_map_network_status
  on mother_api.asset_chain_map (network_id, status);

comment on table mother_api.global_asset is
  'Product-facing chain-agnostic assets known by Iron Burrow for Sentinel search and routing.';

comment on column mother_api.global_asset.slug is
  'Stable lowercase public identifier, for example usdc, gold, bitcoin, ethereum.';

comment on column mother_api.global_asset.aliases is
  'Normalized query aliases for simple deterministic resolver matches.';

comment on type mother_api.global_asset_status is
  'Lifecycle state for chain-agnostic global assets.';

comment on table mother_api.network is
  'Product-facing network catalog, for example bitcoin-mainnet, eth-mainnet, base, mantle.';

comment on column mother_api.network.caip2 is
  'Optional CAIP-2 network identifier, for example eip155:8453.';

comment on table mother_api.asset_chain_map is
  'Mapping between a chain-agnostic global asset and its network-specific native or deployed representation.';

comment on column mother_api.asset_chain_map.is_native is
  'True when the asset is the native currency for the network.';

comment on column mother_api.asset_chain_map.deployment_address is
  'Lowercase deployment address for contract-backed representations; null for native assets.';

comment on column mother_api.asset_chain_map.deployment_block is
  'Known deployment block for contract-backed representations when deliberately verified.';
