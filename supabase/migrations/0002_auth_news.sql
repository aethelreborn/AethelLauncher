
create table if not exists public.auth_users (
    id            uuid primary key default gen_random_uuid(),
    profile_id    uuid not null unique references public.profiles(id) on delete cascade,
    email         text not null unique,
    password_hash text not null,
    role          text not null default 'user' check (role in ('user', 'admin')),
    created_at    timestamptz not null default now(),
    last_login_at timestamptz
);

create unique index if not exists auth_users_email_key
    on public.auth_users (lower(email));

create table if not exists public.auth_refresh_tokens (
    id         uuid primary key default gen_random_uuid(),
    user_id    uuid not null references public.auth_users(id) on delete cascade,
    token_hash text not null unique,
    expires_at timestamptz not null,
    revoked_at timestamptz,
    created_at timestamptz not null default now()
);

create index if not exists auth_refresh_tokens_user_idx
    on public.auth_refresh_tokens (user_id);

create table if not exists public.news (
    id           uuid primary key default gen_random_uuid(),
    title        text not null check (char_length(title) between 1 and 200),
    body         text not null default '',
    url          text,
    published_at timestamptz not null default now(),
    created_at   timestamptz not null default now()
);

create index if not exists news_published_idx
    on public.news (published_at desc);

alter table public.auth_users          enable row level security;
alter table public.auth_refresh_tokens enable row level security;
alter table public.news                enable row level security;


drop policy if exists "news is publicly readable" on public.news;
create policy "news is publicly readable"
    on public.news for select using (true);

comment on table public.auth_users is
    'Platform accounts. Passwords argon2id-hashed; role only via service key.';
comment on table public.auth_refresh_tokens is
    'Rotating refresh tokens, stored SHA-256-hashed. Revoked on logout/refresh.';
