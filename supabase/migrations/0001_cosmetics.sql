-- Aethel Launcher — cosmetics platform schema.
--
-- Runs on plain PostgreSQL as well as Supabase. Everything the launcher needs to
-- show a catalogue, own items, equip them and record purchases.
--
-- Apply with:  psql "$DATABASE_URL" -f supabase/migrations/0001_cosmetics.sql
-- or on Supabase:  supabase db push

create extension if not exists "pgcrypto";

-- ---------------------------------------------------------------------------
-- Profiles
-- ---------------------------------------------------------------------------
-- Keyed by lower-cased username so the launcher's offline identity works with
-- no signup. A Microsoft-linked account later claims the same row.
create table if not exists public.profiles (
    id           uuid primary key default gen_random_uuid(),
    username     text not null,
    username_ci  text not null generated always as (lower(username)) stored,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now()
);

create unique index if not exists profiles_username_ci_key
    on public.profiles (username_ci);

-- ---------------------------------------------------------------------------
-- Cosmetic catalogue
-- ---------------------------------------------------------------------------
create table if not exists public.cosmetics (
    id           uuid primary key default gen_random_uuid(),
    slug         text not null unique,
    name         text not null,
    -- cape | skin | elytra | badge | hud | wings
    kind         text not null,
    -- common | rare | epic | legendary
    rarity       text not null default 'common',
    -- Path inside the public `cosmetics` storage bucket, or an absolute URL.
    asset_url    text not null,
    -- Fallback tint used when the asset image cannot be fetched.
    tint_hex     text not null default '#6C5CE7',
    price_cents  integer not null default 0 check (price_cents >= 0),
    -- Null/never-expiring by default; set for seasonal items.
    available_until timestamptz,
    is_public    boolean not null default true,
    sort_order   integer not null default 0,
    created_at   timestamptz not null default now()
);

create index if not exists cosmetics_kind_idx on public.cosmetics (kind);

-- ---------------------------------------------------------------------------
-- Wallets, inventory, equipped slots
-- ---------------------------------------------------------------------------
create table if not exists public.wallets (
    player_id     uuid primary key references public.profiles(id) on delete cascade,
    balance_cents integer not null default 500 check (balance_cents >= 0),
    updated_at    timestamptz not null default now()
);

create table if not exists public.inventory (
    player_id   uuid not null references public.profiles(id) on delete cascade,
    cosmetic_id uuid not null references public.cosmetics(id) on delete cascade,
    acquired_at timestamptz not null default now(),
    primary key (player_id, cosmetic_id)
);

-- One item equipped per kind, which is what the in-game renderer expects.
create table if not exists public.equipped (
    player_id   uuid not null references public.profiles(id) on delete cascade,
    kind        text not null,
    cosmetic_id uuid not null references public.cosmetics(id) on delete cascade,
    equipped_at timestamptz not null default now(),
    primary key (player_id, kind)
);

-- ---------------------------------------------------------------------------
-- Purchases (idempotent)
-- ---------------------------------------------------------------------------
create table if not exists public.purchases (
    id           uuid primary key default gen_random_uuid(),
    player_id    uuid not null references public.profiles(id) on delete cascade,
    cosmetic_id  uuid not null references public.cosmetics(id) on delete restrict,
    price_cents  integer not null,
    -- Client-supplied key so a retried request never double-charges.
    request_id   text not null unique,
    created_at   timestamptz not null default now()
);

-- ---------------------------------------------------------------------------
-- Row level security
-- ---------------------------------------------------------------------------
alter table public.profiles  enable row level security;
alter table public.cosmetics enable row level security;
alter table public.wallets   enable row level security;
alter table public.inventory enable row level security;
alter table public.equipped  enable row level security;
alter table public.purchases enable row level security;

-- The catalogue is public read-only data.
drop policy if exists "cosmetics are publicly readable" on public.cosmetics;
create policy "cosmetics are publicly readable"
    on public.cosmetics for select using (is_public);

drop policy if exists "profiles are publicly readable" on public.profiles;
drop policy if exists "equipped is publicly readable" on public.equipped;

drop policy if exists "players read their own wallet" on public.wallets;
create policy "players read their own wallet"
    on public.wallets for select using (auth.uid() = player_id);

drop policy if exists "players read their own inventory" on public.inventory;
create policy "players read their own inventory"
    on public.inventory for select using (auth.uid() = player_id);

drop policy if exists "players read their own purchases" on public.purchases;
create policy "players read their own purchases"
    on public.purchases for select using (auth.uid() = player_id);

-- Note: writes (purchase/equip) go through the Axum backend using the
-- service-role key, which is why no INSERT/UPDATE policies are defined here.
-- Granting the anon key write access would let a client mint currency.

-- ---------------------------------------------------------------------------
-- Purchase function — atomic, idempotent, and the only way to spend
-- ---------------------------------------------------------------------------
create or replace function public.purchase_cosmetic(
    p_username   text,
    p_cosmetic   uuid,
    p_request_id text
)
returns table (status text, balance_cents integer, cosmetic_id uuid)
language plpgsql
security definer
set search_path = public, pg_temp
as $$
declare
    v_player  uuid;
    v_price   integer;
    v_balance integer;
    v_existing integer;
begin
    -- Idempotency: a replayed request returns the original outcome.
    select count(*) into v_existing
      from public.purchases pu
      join public.profiles pr on pr.id = pu.player_id
     where pu.request_id = p_request_id;

    if v_existing > 0 then
        select w.balance_cents, pu.cosmetic_id into v_balance, v_player
          from public.purchases pu
          join public.wallets w on w.player_id = pu.player_id
         where pu.request_id = p_request_id
         limit 1;
        return query select 'already_purchased'::text, v_balance, v_player;
        return;
    end if;

    insert into public.profiles (username)
    values (p_username)
    on conflict (username_ci) do update set updated_at = now()
    returning id into v_player;

    insert into public.wallets (player_id) values (v_player)
    on conflict (player_id) do nothing;

    select price_cents into v_price from public.cosmetics where id = p_cosmetic;
    if v_price is null then
        return query select 'not_found'::text, null::integer, null::uuid;
        return;
    end if;

    -- Free items skip the wallet entirely.
    if v_price = 0 then
        insert into public.inventory (player_id, cosmetic_id)
        values (v_player, p_cosmetic)
        on conflict do nothing;
        insert into public.purchases (player_id, cosmetic_id, price_cents, request_id)
        values (v_player, p_cosmetic, 0, p_request_id);
        select balance_cents into v_balance from public.wallets where player_id = v_player;
        return query select 'purchased'::text, v_balance, p_cosmetic;
        return;
    end if;

    update public.wallets
       set balance_cents = balance_cents - v_price,
           updated_at = now()
     where player_id = v_player
       and balance_cents >= v_price
    returning balance_cents into v_balance;

    if v_balance is null then
        select w.balance_cents into v_balance from public.wallets w where w.player_id = v_player;
        return query select 'insufficient_funds'::text, v_balance, p_cosmetic;
        return;
    end if;

    insert into public.inventory (player_id, cosmetic_id)
    values (v_player, p_cosmetic)
    on conflict do nothing;

    insert into public.purchases (player_id, cosmetic_id, price_cents, request_id)
    values (v_player, p_cosmetic, v_price, p_request_id);

    return query select 'purchased'::text, v_balance, p_cosmetic;
end;
$$;

revoke execute on function public.purchase_cosmetic(text, uuid, text) from public;

-- ---------------------------------------------------------------------------
-- Seed catalogue
-- ---------------------------------------------------------------------------
insert into public.cosmetics (slug, name, kind, rarity, asset_url, tint_hex, price_cents, sort_order)
values
    ('cape-aethel',       'Aethel Cape',        'cape',   'common',    'capes/aethel.png',        '#6C5CE7', 0,    10),
    ('cape-aurora',       'Aurora Cape',        'cape',   'rare',      'capes/aurora.png',        '#00D2FF', 250,  20),
    ('cape-ember',        'Ember Cape',         'cape',   'epic',      'capes/ember.png',         '#E74C3C', 500,  30),
    ('wings-void',        'Void Wings',         'wings',  'legendary', 'wings/void.png',          '#8E44AD', 900,  40),
    ('wings-prism',       'Prism Wings',        'wings',  'epic',      'wings/prism.png',         '#00D2FF', 700,  50),
    ('badge-founder',     'Founder Badge',      'badge',  'legendary', 'badges/founder.png',      '#F1C40F', 0,    60),
    ('badge-early',       'Early Adopter',      'badge',  'rare',      'badges/early.png',        '#2ECC71', 0,    70),
    ('badge-bugfinder',   'Bug Finder',         'badge',  'epic',      'badges/bugfinder.png',    '#E67E22', 0,    80),
    ('hud-minimal',       'Minimal HUD',        'hud',    'common',    'hud/minimal.png',         '#95A5A6', 0,    90),
    ('hud-neon',          'Neon HUD',           'hud',    'rare',      'hud/neon.png',            '#00D2FF', 300, 100),
    ('elytra-dragon',     'Dragon Elytra',      'elytra', 'legendary', 'elytra/dragon.png',       '#111111', 1200, 110),
    ('skin-shadow',       'Shadow Skin Accent', 'skin',   'rare',      'skins/shadow.png',        '#2C3E50', 200, 120)
on conflict (slug) do nothing;

-- Grant a starting balance to the first 10k players (handled in app code via
-- the wallets default of 500 cents); documented here for clarity.
comment on table public.wallets is
    'Player currency. Default balance is a welcome grant; writes only via backend service role.';
