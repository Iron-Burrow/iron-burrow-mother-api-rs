create extension if not exists pgcrypto;

create schema if not exists mother_api;

comment on schema mother_api is
  'Mother API owned application schema. Contains Sentinel-facing global asset catalog data, not indexer-owned runtime tables.';

create table if not exists mother_api.global_assets (
  id uuid primary key default gen_random_uuid(),
  slug text not null,
  symbol text not null,
  name text not null,
  asset_kind text not null default 'crypto',
  category text,
  canonical_path text not null,
  aliases text[] not null default '{}',
  metadata jsonb not null default '{}'::jsonb,
  status text not null default 'active',
  sort_order integer not null default 1000,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create unique index if not exists uq_mother_api_global_assets_slug_lower
  on mother_api.global_assets (lower(slug));

create unique index if not exists uq_mother_api_global_assets_active_symbol_lower
  on mother_api.global_assets (lower(symbol))
  where status = 'active';

create index if not exists idx_mother_api_global_assets_status_sort
  on mother_api.global_assets (status, sort_order);

create index if not exists idx_mother_api_global_assets_slug_lower
  on mother_api.global_assets (lower(slug));

create index if not exists idx_mother_api_global_assets_symbol_lower
  on mother_api.global_assets (lower(symbol));

create index if not exists idx_mother_api_global_assets_name_lower
  on mother_api.global_assets (lower(name));

create index if not exists idx_mother_api_global_assets_aliases_gin
  on mother_api.global_assets using gin (aliases);

comment on table mother_api.global_assets is
  'Product-facing global assets known by Iron Burrow for Sentinel search and routing.';

comment on column mother_api.global_assets.slug is
  'Stable lowercase public identifier, for example usdc, gold, bitcoin, ethereum.';

comment on column mother_api.global_assets.aliases is
  'Normalized query aliases for simple deterministic resolver matches.';
