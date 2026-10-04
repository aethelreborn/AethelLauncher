
create extension if not exists "pgcrypto";

create table if not exists public.profiles (
    id           uuid primary key default gen_random_uuid(),
    username     text not null,
    username_ci  text not null generated always as (lower(username)) stored,
    created_at   timestamptz not null default now(),
    updated_at   timestamptz not null default now()
);

create unique index if not exists profiles_username_ci_key
    on public.profiles (username_ci);

create table if not exists public.cosmetics (
    id           uuid primary key default gen_random_uuid(),
    slug         text not null unique,
    name         text not null,
    kind         text not null,
    rarity       text not null default 'common',
    asset_url    text not null,
    tint_hex     text not null default '#6C5CE7',
    price_cents  integer not null default 0 check (price_cents >= 0),
    available_until timestamptz,
    is_public    boolean not null default true,
    sort_order   integer not null default 0,
    created_at   timestamptz not null default now()
);

create index if not exists cosmetics_kind_idx on public.cosmetics (kind);

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

create table if not exists public.equipped (
    player_id   uuid not null references public.profiles(id) on delete cascade,
    kind        text not null,
    cosmetic_id uuid not null references public.cosmetics(id) on delete cascade,
    equipped_at timestamptz not null default now(),
    primary key (player_id, kind)
);

create table if not exists public.purchases (
    id           uuid primary key default gen_random_uuid(),
    player_id    uuid not null references public.profiles(id) on delete cascade,
    cosmetic_id  uuid not null references public.cosmetics(id) on delete restrict,
    price_cents  integer not null,
    request_id   text not null unique,
    created_at   timestamptz not null default now()
);

alter table public.profiles  enable row level security;
alter table public.cosmetics enable row level security;
alter table public.wallets   enable row level security;
alter table public.inventory enable row level security;
alter table public.equipped  enable row level security;
alter table public.purchases enable row level security;

drop policy if exists "cosmetics are publicly readable" on public.cosmetics;
create policy "cosmetics are publicly readable"
    on public.cosmetics for select using (is_public);

drop policy if exists "profiles are publicly readable" on public.profiles;
create policy "profiles are publicly readable"
    on public.profiles for select using (true);

do $$
begin
    if to_regprocedure('auth.uid()') is null then
        return;
    end if;

    execute 'drop policy if exists "players read their own wallet" on public.wallets';
    execute 'create policy "players read their own wallet"
                on public.wallets for select using (auth.uid() = player_id)';

    execute 'drop policy if exists "players read their own inventory" on public.inventory';
    execute 'create policy "players read their own inventory"
                on public.inventory for select using (auth.uid() = player_id)';

    execute 'drop policy if exists "equipped is publicly readable" on public.equipped';
    execute 'create policy "equipped is publicly readable"
                on public.equipped for select using (true)';

    execute 'drop policy if exists "players read their own purchases" on public.purchases';
    execute 'create policy "players read their own purchases"
                on public.purchases for select using (auth.uid() = player_id)';
end $$;


create or replace function public.purchase_cosmetic(
    p_username   text,
    p_cosmetic   uuid,
    p_request_id text
)
returns table (status text, balance_cents integer, cosmetic_id uuid)
language plpgsql
security definer
as $$
declare
    v_player  uuid;
    v_price   integer;
    v_balance integer;
    v_existing integer;
begin
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

    if v_price = 0 then
        insert into public.inventory (player_id, cosmetic_id)
        values (v_player, p_cosmetic)
        on conflict do nothing;
        insert into public.purchases (player_id, cosmetic_id, price_cents, request_id)
        values (v_player, p_cosmetic, 0, p_request_id);
        select w.balance_cents into v_balance
          from public.wallets w where w.player_id = v_player;
        return query select 'purchased'::text, v_balance, p_cosmetic;
        return;
    end if;

    update public.wallets w
       set balance_cents = w.balance_cents - v_price,
           updated_at = now()
     where w.player_id = v_player
       and w.balance_cents >= v_price
    returning w.balance_cents into v_balance;

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

comment on table public.wallets is
    'Player currency. Default balance is a welcome grant; writes only via backend service role.';
