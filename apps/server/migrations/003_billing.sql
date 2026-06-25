CREATE TABLE IF NOT EXISTS billing_plans (
    id VARCHAR(32) PRIMARY KEY,
    name TEXT NOT NULL,
    tier VARCHAR(16) NOT NULL,
    description TEXT NOT NULL,
    cloud_enabled BOOLEAN NOT NULL DEFAULT true,
    version_history_days INTEGER,
    max_devices INTEGER,
    max_attachment_bytes BIGINT NOT NULL,
    sync_priority VARCHAR(16) NOT NULL DEFAULT 'standard',
    checkout_cta TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS billing_prices (
    id VARCHAR(32) PRIMARY KEY,
    plan_id VARCHAR(32) NOT NULL REFERENCES billing_plans(id) ON DELETE CASCADE,
    provider VARCHAR(32) NOT NULL,
    provider_price_id VARCHAR(128) NOT NULL,
    billing_interval VARCHAR(16) NOT NULL,
    currency VARCHAR(8) NOT NULL,
    unit_amount INTEGER NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_billing_prices_provider_price
    ON billing_prices(provider, provider_price_id);

CREATE TABLE IF NOT EXISTS subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    plan_id VARCHAR(32) NOT NULL REFERENCES billing_plans(id) ON DELETE RESTRICT,
    price_id VARCHAR(32) NOT NULL REFERENCES billing_prices(id) ON DELETE RESTRICT,
    provider VARCHAR(32) NOT NULL,
    provider_customer_id VARCHAR(128),
    provider_subscription_id VARCHAR(128),
    status VARCHAR(32) NOT NULL,
    cancel_at_period_end BOOLEAN NOT NULL DEFAULT false,
    current_period_start TIMESTAMPTZ,
    current_period_end TIMESTAMPTZ,
    trial_ends_at TIMESTAMPTZ,
    canceled_at TIMESTAMPTZ,
    management_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_subscriptions_provider_subscription
    ON subscriptions(provider, provider_subscription_id);

CREATE INDEX IF NOT EXISTS idx_subscriptions_user
    ON subscriptions(user_id, current_period_end DESC, created_at DESC);

CREATE TABLE IF NOT EXISTS entitlements (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key VARCHAR(64) NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    limit_value BIGINT,
    source VARCHAR(32) NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY(user_id, key)
);

CREATE TABLE IF NOT EXISTS usage_counters (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key VARCHAR(64) NOT NULL,
    used BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY(user_id, key)
);

CREATE TABLE IF NOT EXISTS billing_events (
    id BIGSERIAL PRIMARY KEY,
    provider VARCHAR(32) NOT NULL,
    event_id VARCHAR(128) NOT NULL,
    event_name VARCHAR(128) NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}',
    processed BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    processed_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_billing_events_provider_event
    ON billing_events(provider, event_id);

INSERT INTO billing_plans
    (id, name, tier, description, cloud_enabled, version_history_days, max_devices, max_attachment_bytes, sync_priority, checkout_cta)
VALUES
    ('free', 'Cloud Free', 'free', '基础云同步，适合轻量个人使用。', true, 7, 2, 536870912, 'standard', '当前套餐'),
    ('pro', 'Cloud Pro', 'pro', '完整云同步、扩容附件和无限版本历史。', true, NULL, NULL, 21474836480, 'priority', '升级到 Pro')
ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    tier = EXCLUDED.tier,
    description = EXCLUDED.description,
    cloud_enabled = EXCLUDED.cloud_enabled,
    version_history_days = EXCLUDED.version_history_days,
    max_devices = EXCLUDED.max_devices,
    max_attachment_bytes = EXCLUDED.max_attachment_bytes,
    sync_priority = EXCLUDED.sync_priority,
    checkout_cta = EXCLUDED.checkout_cta;

INSERT INTO billing_prices
    (id, plan_id, provider, provider_price_id, billing_interval, currency, unit_amount, is_active)
VALUES
    ('pro-monthly', 'pro', 'lemonsqueezy', 'pro-monthly', 'month', 'USD', 599, true),
    ('pro-yearly', 'pro', 'lemonsqueezy', 'pro-yearly', 'year', 'USD', 5990, true)
ON CONFLICT (id) DO UPDATE SET
    plan_id = EXCLUDED.plan_id,
    provider = EXCLUDED.provider,
    provider_price_id = EXCLUDED.provider_price_id,
    billing_interval = EXCLUDED.billing_interval,
    currency = EXCLUDED.currency,
    unit_amount = EXCLUDED.unit_amount,
    is_active = EXCLUDED.is_active;
