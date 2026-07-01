do $$
begin
  if exists (
    select 1
    from mother_api.global_asset
    where slug <> lower(btrim(slug))
      or slug = ''
  ) then
    raise exception 'global_asset contains non-normalized slug values';
  end if;

  if exists (
    select 1
    from mother_api.global_asset
    group by slug
    having count(*) > 1
  ) then
    raise exception 'global_asset contains duplicate slug identities';
  end if;

  if exists (
    select 1
    from mother_api.network
    where slug <> lower(btrim(slug))
      or slug = ''
  ) then
    raise exception 'network contains non-normalized slug values';
  end if;

  if exists (
    select 1
    from mother_api.network
    group by slug
    having count(*) > 1
  ) then
    raise exception 'network contains duplicate slug identities';
  end if;

  if exists (
    select 1
    from mother_api.asset_chain_map
    group by asset_id, network_id
    having count(*) > 1
  ) then
    raise exception 'asset_chain_map contains duplicate asset/network identities';
  end if;

  if not exists (
    select 1
    from pg_constraint constraint_record
    join pg_class relation_record
      on relation_record.oid = constraint_record.conrelid
    join pg_namespace namespace_record
      on namespace_record.oid = relation_record.relnamespace
    where namespace_record.nspname = 'mother_api'
      and relation_record.relname = 'global_asset'
      and constraint_record.conname = 'global_asset_slug_normalized'
  ) then
    alter table mother_api.global_asset
      add constraint global_asset_slug_normalized
      check (slug = lower(btrim(slug)) and slug <> '');
  end if;

  if not exists (
    select 1
    from pg_constraint constraint_record
    join pg_class relation_record
      on relation_record.oid = constraint_record.conrelid
    join pg_namespace namespace_record
      on namespace_record.oid = relation_record.relnamespace
    where namespace_record.nspname = 'mother_api'
      and relation_record.relname = 'global_asset'
      and constraint_record.conname = 'global_asset_slug_unique'
  ) then
    alter table mother_api.global_asset
      add constraint global_asset_slug_unique unique (slug);
  end if;

  if not exists (
    select 1
    from pg_constraint constraint_record
    join pg_class relation_record
      on relation_record.oid = constraint_record.conrelid
    join pg_namespace namespace_record
      on namespace_record.oid = relation_record.relnamespace
    where namespace_record.nspname = 'mother_api'
      and relation_record.relname = 'network'
      and constraint_record.conname = 'network_slug_normalized'
  ) then
    alter table mother_api.network
      add constraint network_slug_normalized
      check (slug = lower(btrim(slug)) and slug <> '');
  end if;

  if not exists (
    select 1
    from pg_constraint constraint_record
    join pg_class relation_record
      on relation_record.oid = constraint_record.conrelid
    join pg_namespace namespace_record
      on namespace_record.oid = relation_record.relnamespace
    where namespace_record.nspname = 'mother_api'
      and relation_record.relname = 'network'
      and constraint_record.conname = 'network_slug_unique'
  ) then
    alter table mother_api.network
      add constraint network_slug_unique unique (slug);
  end if;

  if not exists (
    select 1
    from pg_constraint constraint_record
    join pg_class relation_record
      on relation_record.oid = constraint_record.conrelid
    join pg_namespace namespace_record
      on namespace_record.oid = relation_record.relnamespace
    where namespace_record.nspname = 'mother_api'
      and relation_record.relname = 'asset_chain_map'
      and constraint_record.conname = 'asset_chain_map_asset_network_unique'
  ) then
    alter table mother_api.asset_chain_map
      add constraint asset_chain_map_asset_network_unique unique (asset_id, network_id);
  end if;
end $$;
