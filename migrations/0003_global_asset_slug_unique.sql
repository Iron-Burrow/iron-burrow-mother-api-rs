do $$
begin
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
end $$;
