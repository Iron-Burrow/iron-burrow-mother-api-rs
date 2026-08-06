create table if not exists mother_api.defi_protocol (
  id uuid primary key default gen_random_uuid(),
  slug text not null unique,
  network_id uuid not null references mother_api.network (id) on delete restrict,
  family text not null,
  adapter_kind text not null,
  adapter_version text not null,
  enabled boolean not null default false,
  verified boolean not null default false,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint defi_protocol_slug_normalized check (slug = lower(btrim(slug)) and slug <> '')
);

create table if not exists mother_api.defi_protocol_target (
  id uuid primary key default gen_random_uuid(),
  defi_protocol_id uuid not null references mother_api.defi_protocol (id) on delete cascade,
  target_key text not null,
  target_kind text not null,
  address text not null,
  asset_chain_map_id uuid references mother_api.asset_chain_map (id) on delete restrict,
  configuration jsonb not null default '{}'::jsonb,
  enabled boolean not null default false,
  verified boolean not null default false,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint defi_protocol_target_key_nonempty check (btrim(target_key) <> ''),
  constraint defi_protocol_target_kind_supported check (target_kind in ('pool', 'reserve')),
  constraint defi_protocol_target_address_lowercase_evm check (address ~ '^0x[0-9a-f]{40}$'),
  constraint defi_protocol_target_asset_binding check (
    (target_kind = 'pool' and asset_chain_map_id is null)
    or (target_kind = 'reserve' and asset_chain_map_id is not null)
  ),
  constraint defi_protocol_target_configuration_bounded check (configuration = '{}'::jsonb),
  unique (defi_protocol_id, target_key)
);

create or replace function mother_api.verify_defi_protocol_target_network()
returns trigger
language plpgsql
as $$
declare
  protocol_network_id uuid;
  asset_network_id uuid;
  asset_address text;
begin
  select network_id into protocol_network_id
  from mother_api.defi_protocol
  where id = new.defi_protocol_id;

  if new.target_kind = 'reserve' then
    select network_id, deployment_address into asset_network_id, asset_address
    from mother_api.asset_chain_map
    where id = new.asset_chain_map_id;

    if asset_network_id is distinct from protocol_network_id
       or lower(asset_address) is distinct from new.address then
      raise exception 'DeFi protocol reserve target must match its protocol network and asset deployment';
    end if;
  end if;
  return new;
end;
$$;

create trigger verify_defi_protocol_target_network
before insert or update of defi_protocol_id, target_kind, address, asset_chain_map_id
on mother_api.defi_protocol_target
for each row execute function mother_api.verify_defi_protocol_target_network();

create index if not exists idx_defi_protocol_enabled_slug
  on mother_api.defi_protocol (slug)
  where enabled and verified;
create index if not exists idx_defi_protocol_target_protocol
  on mother_api.defi_protocol_target (defi_protocol_id, target_kind)
  where enabled and verified;
