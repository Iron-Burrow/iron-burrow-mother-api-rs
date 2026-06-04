do $$
begin
  create type mother_api.billing_currency as enum (
    'USD_MICRO',
    'BTC_SATS'
  );
exception
  when duplicate_object then null;
end $$;

do $$
begin
  create type mother_api.api_key_type as enum (
    'DEMO_LIKE',
    'ONE_TIME_API',
    'SHREW_SUBSCRIPTION'
  );
exception
  when duplicate_object then null;
end $$;

create table if not exists mother_api.accounts (
  id uuid primary key default gen_random_uuid(),
  preferred_currency mother_api.billing_currency not null default 'USD_MICRO',
  status text not null default 'active',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table if not exists mother_api.api_keys (
  id uuid primary key default gen_random_uuid(),
  account_id uuid not null references mother_api.accounts (id) on delete cascade,
  key_hash text not null,
  key_prefix text not null,
  key_type mother_api.api_key_type not null,
  label text,
  status text not null default 'active',
  created_at timestamptz not null default now(),
  revoked_at timestamptz,
  constraint api_keys_key_hash_not_empty check (btrim(key_hash) <> ''),
  constraint api_keys_key_prefix_not_empty check (btrim(key_prefix) <> '')
);

create table if not exists mother_api.account_balances (
  account_id uuid not null references mother_api.accounts (id) on delete cascade,
  currency mother_api.billing_currency not null,
  available_amount_minor bigint not null default 0,
  updated_at timestamptz not null default now(),
  primary key (account_id, currency),
  constraint account_balances_available_non_negative check (available_amount_minor >= 0)
);

create table if not exists mother_api.usage_price_catalog (
  id uuid primary key default gen_random_uuid(),
  operation text not null,
  price_currency mother_api.billing_currency not null,
  price_amount_minor bigint not null,
  pricing_unit text not null default 'request',
  max_window_days integer,
  status text not null default 'active',
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint usage_price_catalog_operation_not_empty check (btrim(operation) <> ''),
  constraint usage_price_catalog_amount_non_negative check (price_amount_minor >= 0)
);

create table if not exists mother_api.usage_ledger (
  id uuid primary key default gen_random_uuid(),
  account_id uuid references mother_api.accounts (id) on delete set null,
  api_key_id uuid references mother_api.api_keys (id) on delete set null,
  operation text not null,
  currency mother_api.billing_currency not null,
  amount_minor bigint not null,
  request_id text,
  status text not null default 'quoted',
  metadata_json jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now(),
  constraint usage_ledger_operation_not_empty check (btrim(operation) <> '')
);

create unique index if not exists uq_mother_api_api_keys_hash
  on mother_api.api_keys (key_hash);

create index if not exists idx_mother_api_api_keys_account_status
  on mother_api.api_keys (account_id, status);

create unique index if not exists uq_mother_api_usage_price_catalog_active
  on mother_api.usage_price_catalog (
    operation,
    price_currency,
    coalesce(max_window_days, -1)
  )
  where status = 'active';

create index if not exists idx_mother_api_usage_ledger_account_created
  on mother_api.usage_ledger (account_id, created_at desc);

comment on table mother_api.accounts is
  'Mother API metering account owner. API keys are credentials; accounts own balances and plans.';

comment on table mother_api.api_keys is
  'Access credentials for demo-like, one-time, and subscription-style Mother API usage.';

comment on table mother_api.account_balances is
  'Integer minor-unit account balances. 1 USD = 1,000,000 USD_MICRO; 1 BTC = 100,000,000 sats.';

comment on table mother_api.usage_price_catalog is
  'Alpha operation pricing in real monetary minor units, not abstract credits.';

comment on table mother_api.usage_ledger is
  'Future usage ledger foundation. This alpha slice quotes usage but does not debit or fake ledger records.';

insert into mother_api.usage_price_catalog (
  operation,
  price_currency,
  price_amount_minor,
  pricing_unit,
  max_window_days,
  status,
  updated_at
)
values
  ('price.latest', 'USD_MICRO', 100, 'request', null, 'active', now()),
  ('price.latest', 'BTC_SATS', 1, 'request', null, 'active', now()),
  ('signal.price_stats', 'USD_MICRO', 500, 'request', 7, 'active', now()),
  ('signal.price_stats', 'BTC_SATS', 1, 'request', 7, 'active', now()),
  ('signal.price_stats', 'USD_MICRO', 1500, 'request', 31, 'active', now()),
  ('signal.price_stats', 'BTC_SATS', 5, 'request', 31, 'active', now()),
  ('signal.price_trend', 'USD_MICRO', 1000, 'request', 7, 'active', now()),
  ('signal.price_trend', 'BTC_SATS', 3, 'request', 7, 'active', now()),
  ('signal.price_trend', 'USD_MICRO', 3000, 'request', 31, 'active', now()),
  ('signal.price_trend', 'BTC_SATS', 10, 'request', 31, 'active', now())
on conflict do nothing;
