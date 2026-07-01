create table if not exists mother_api.api_key_policy (
  api_key_id uuid primary key
    references mother_api.api_key (id) on delete cascade,

  requests_per_minute integer not null default 60,
  requests_per_day integer not null default 5000,

  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),

  constraint api_key_policy_requests_per_minute_non_negative
    check (requests_per_minute >= 0),

  constraint api_key_policy_requests_per_day_non_negative
    check (requests_per_day >= 0),

  constraint api_key_policy_timestamps_sane
    check (updated_at >= created_at)
);

create table if not exists mother_api.api_key_usage_daily (
  api_key_id uuid not null
    references mother_api.api_key (id) on delete cascade,

  usage_date date not null,

  accepted_requests bigint not null default 0,
  rate_limited_requests bigint not null default 0,
  successful_responses bigint not null default 0,
  client_error_responses bigint not null default 0,
  server_error_responses bigint not null default 0,

  last_used_at timestamptz,

  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),

  primary key (api_key_id, usage_date),

  constraint api_key_usage_daily_counts_non_negative
    check (
      accepted_requests >= 0
      and rate_limited_requests >= 0
      and successful_responses >= 0
      and client_error_responses >= 0
      and server_error_responses >= 0
    ),

  constraint api_key_usage_daily_timestamps_sane
    check (
      updated_at >= created_at
      and (last_used_at is null or last_used_at >= created_at)
    )
);

comment on table mother_api.api_key_policy is
  'Runtime policy rows for issued API keys. Migrations and reference data must not create real customer policies.';

comment on table mother_api.api_key_usage_daily is
  'Runtime daily usage counters for issued API keys. Migrations and reference data must not create real customer usage rows.';
