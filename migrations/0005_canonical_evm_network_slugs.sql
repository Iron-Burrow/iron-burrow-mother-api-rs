do $$
declare
  mapping record;
  legacy_count bigint;
  canonical_count bigint;
begin
  lock table mother_api.network in share row exclusive mode;

  for mapping in
    select *
    from (
      values
        ('base', 'base-mainnet'),
        ('mantle', 'mantle-mainnet'),
        ('arbitrum-one', 'arbitrum-mainnet')
    ) as mappings(legacy_slug, canonical_slug)
  loop
    select count(*)
    into legacy_count
    from mother_api.network
    where status = 'active'
      and slug = mapping.legacy_slug;

    if legacy_count <> 1 then
      raise exception
        'expected exactly one active network row for legacy slug %, found %',
        mapping.legacy_slug,
        legacy_count;
    end if;

    select count(*)
    into canonical_count
    from mother_api.network
    where slug = mapping.canonical_slug;

    if canonical_count <> 0 then
      raise exception
        'canonical network slug % conflicts with an existing row',
        mapping.canonical_slug;
    end if;

    update mother_api.network
    set
      slug = mapping.canonical_slug,
      updated_at = now()
    where status = 'active'
      and slug = mapping.legacy_slug;
  end loop;

  if exists (
    select 1
    from mother_api.network
    where status = 'active'
      and slug in ('base', 'mantle', 'arbitrum-one')
  ) then
    raise exception 'active legacy EVM network slugs remain after canonical migration';
  end if;

  if (
    select count(*)
    from mother_api.network
    where status = 'active'
      and family = 'evm'
      and chain_id > 0
      and slug in ('base-mainnet', 'mantle-mainnet', 'arbitrum-mainnet')
  ) <> 3 then
    raise exception 'canonical EVM network postconditions were not satisfied';
  end if;
end $$;

comment on table mother_api.network is
  'Product-facing network catalog using canonical slugs such as bitcoin-mainnet, eth-mainnet, base-mainnet, mantle-mainnet, and arbitrum-mainnet.';
