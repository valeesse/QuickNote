use crate::error::AppError;
use crate::middleware::AuthUser;
use crate::models::{
    AccountSummary, BillingPortalResponse, CheckoutSessionResponse, CreateCheckoutRequest,
    LemonSqueezyWebhookPayload, SubscriptionRecord, SubscriptionSummary, UsageMetric, User,
    UserResponse,
};
use crate::routes::notes::prune_versions_to_plan_window;
use crate::AppState;
use axum::{body::Bytes, extract::State, http::HeaderMap, response::IntoResponse, Json};
use chrono::{DateTime, Utc};
use hex::encode as hex_encode;
use hmac::{Hmac, Mac};
use quicknote_protocol::{BillingPlan, BillingPrice, EntitlementSummary};
use serde_json::{json, Value};
use sha2::Sha256;
use std::sync::Arc;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

const FREE_PLAN_ID: &str = "free";
const PRO_MONTHLY_PRICE_ID: &str = "pro-monthly";
const PRO_YEARLY_PRICE_ID: &str = "pro-yearly";
const ACTIVE_DEVICE_WINDOW_DAYS: i64 = 45;

#[derive(Debug, Clone)]
pub struct AccountAccess {
    pub user: UserResponse,
    pub plans: Vec<BillingPlan>,
    pub prices: Vec<BillingPrice>,
    pub subscription: Option<SubscriptionSummary>,
    pub entitlements: Vec<EntitlementSummary>,
    pub usage: Vec<UsageMetric>,
}

impl AccountAccess {
    pub fn limit_for(&self, key: &str) -> Option<i64> {
        self.entitlements
            .iter()
            .find(|item| item.key == key)
            .and_then(|item| item.limit)
    }

    pub fn usage_for(&self, key: &str) -> Option<i64> {
        self.usage
            .iter()
            .find(|item| item.key == key)
            .map(|item| item.used)
    }
}

pub async fn account_summary(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<AccountSummary>, AppError> {
    let access = resolve_account_access(&state, user_id).await?;
    persist_account_projection(&state, user_id, &access.entitlements, &access.usage).await?;
    Ok(Json(AccountSummary {
        user: access.user,
        plans: access.plans,
        prices: access.prices,
        subscription: access.subscription,
        entitlements: access.entitlements,
        usage: access.usage,
        billing_provider: state.config.billing_provider.clone(),
        billing_ready: billing_ready(&state),
    }))
}

pub async fn create_checkout(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
    Json(req): Json<CreateCheckoutRequest>,
) -> Result<Json<CheckoutSessionResponse>, AppError> {
    let access = resolve_account_access(&state, user_id).await?;
    let price = access
        .prices
        .iter()
        .find(|item| item.id == req.price_id && item.is_active)
        .cloned()
        .ok_or_else(|| AppError::BadRequest("Unknown billing price".into()))?;
    if price.plan_id == FREE_PLAN_ID {
        return Err(AppError::BadRequest(
            "The free plan does not require checkout".into(),
        ));
    }
    let user = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, created_at FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(state.db.inner())
    .await?
    .ok_or(AppError::Auth)?;
    let checkout_url = create_lemonsqueezy_checkout(&state, &user, &price).await?;
    Ok(Json(CheckoutSessionResponse { checkout_url }))
}

pub async fn billing_portal(
    State(state): State<Arc<AppState>>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<BillingPortalResponse>, AppError> {
    let access = resolve_account_access(&state, user_id).await?;
    if let Some(subscription) = access.subscription {
        if let Some(management_url) = subscription.management_url {
            return Ok(Json(BillingPortalResponse { management_url }));
        }
    }
    if let Some(management_url) = state.config.billing_manage_url.clone() {
        return Ok(Json(BillingPortalResponse { management_url }));
    }
    let support_email = state
        .config
        .billing_support_email
        .clone()
        .unwrap_or_else(|| "support@example.com".to_string());
    Ok(Json(BillingPortalResponse {
        management_url: format!("mailto:{support_email}?subject=QuickNote%20billing"),
    }))
}

pub async fn lemonsqueezy_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    verify_lemonsqueezy_signature(&state, &headers, &body)?;
    let payload: LemonSqueezyWebhookPayload = serde_json::from_slice(&body)
        .map_err(|error| AppError::BadRequest(format!("Invalid webhook payload: {error}")))?;
    let event_name = payload
        .meta
        .get("event_name")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let event_id = payload
        .data
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();

    let insert_result = sqlx::query(
        "INSERT INTO billing_events (provider, event_id, event_name, payload, processed)
         VALUES ($1, $2, $3, $4, false)
         ON CONFLICT (provider, event_id) DO NOTHING",
    )
    .bind("lemonsqueezy")
    .bind(&event_id)
    .bind(&event_name)
    .bind(serde_json::to_value(&payload).map_err(|error| AppError::Internal(error.to_string()))?)
    .execute(state.db.inner())
    .await?;
    if insert_result.rows_affected() == 0 {
        return Ok(Json(json!({ "ok": true, "duplicate": true })));
    }

    process_lemonsqueezy_event(&state, &event_name, &payload).await?;

    sqlx::query(
        "UPDATE billing_events
         SET processed = true, processed_at = NOW(), updated_at = NOW()
         WHERE provider = $1 AND event_id = $2",
    )
    .bind("lemonsqueezy")
    .bind(&event_id)
    .execute(state.db.inner())
    .await?;

    Ok(Json(json!({ "ok": true })))
}

pub async fn resolve_account_access(
    state: &AppState,
    user_id: Uuid,
) -> Result<AccountAccess, AppError> {
    let user: User =
        sqlx::query_as("SELECT id, email, password_hash, created_at FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(state.db.inner())
            .await?
            .ok_or(AppError::Auth)?;

    let plans = load_plans(state).await?;
    let prices = load_prices(state).await?;
    let subscription_records = fetch_subscription_records(state, user_id).await?;
    let subscription = subscription_records.first().map(to_subscription_summary);
    let active_plan_id = effective_subscription(&subscription_records)
        .map(|item| item.plan_id.as_str())
        .unwrap_or(FREE_PLAN_ID);
    let plan = plans
        .iter()
        .find(|item| item.id == active_plan_id)
        .cloned()
        .or_else(|| plans.iter().find(|item| item.id == FREE_PLAN_ID).cloned())
        .ok_or_else(|| AppError::Internal("Billing plans are not seeded".into()))?;
    let entitlements = entitlements_for_plan(&plan);
    let usage = usage_for_user(state, user_id, &plan).await?;

    Ok(AccountAccess {
        user: UserResponse {
            id: user.id,
            email: user.email,
        },
        plans,
        prices,
        subscription,
        entitlements,
        usage,
    })
}

pub async fn ensure_attachment_quota(
    state: &AppState,
    user_id: Uuid,
    incoming_size: i64,
) -> Result<AccountAccess, AppError> {
    let user = fetch_user_identity(state, user_id).await?;
    let plan = load_effective_plan(state, user_id).await?;
    let used = attachment_usage_bytes(state, user_id).await?;
    let entitlements = entitlements_for_plan(&plan);
    let usage = vec![UsageMetric {
        key: "attachment_bytes".into(),
        used,
        limit: Some(plan.max_attachment_bytes),
        unit: "bytes".into(),
    }];
    let access = AccountAccess {
        user,
        plans: Vec::new(),
        prices: Vec::new(),
        subscription: None,
        entitlements,
        usage,
    };
    if let Some(limit) = access.limit_for("attachment_bytes") {
        let used = access.usage_for("attachment_bytes").unwrap_or(0);
        if used + incoming_size > limit {
            return Err(AppError::BadRequest(format!(
                "Attachment storage quota exceeded. Used {used} bytes of {limit} bytes."
            )));
        }
    }
    Ok(access)
}

pub async fn ensure_device_allowed(
    state: &AppState,
    user_id: Uuid,
    device_id: &str,
) -> Result<(), AppError> {
    ensure_user_exists(state, user_id).await?;
    let plan = load_effective_plan(state, user_id).await?;
    let Some(limit) = plan.max_devices.map(i64::from) else {
        return Ok(());
    };
    let already_known: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sync_cursors WHERE user_id = $1 AND device_id = $2)",
    )
    .bind(user_id)
    .bind(device_id)
    .fetch_one(state.db.inner())
    .await?;
    if already_known {
        return Ok(());
    }
    let used: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM sync_cursors
         WHERE user_id = $1 AND updated_at >= $2",
    )
    .bind(user_id)
    .bind(active_device_cutoff())
    .fetch_one(state.db.inner())
    .await?;
    if used >= limit {
        return Err(AppError::BadRequest(format!(
            "The current cloud plan supports up to {limit} devices. Upgrade to Pro to add more devices."
        )));
    }
    Ok(())
}

pub async fn version_history_cutoff(
    state: &AppState,
    user_id: Uuid,
) -> Result<Option<String>, AppError> {
    ensure_user_exists(state, user_id).await?;
    let plan = load_effective_plan(state, user_id).await?;
    let days = plan.version_history_days.map(i64::from);
    Ok(days.map(|value| {
        (Utc::now() - chrono::Duration::days(value))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }))
}

fn entitlements_for_plan(plan: &BillingPlan) -> Vec<EntitlementSummary> {
    vec![
        EntitlementSummary {
            key: "cloud_sync".into(),
            enabled: plan.cloud_enabled,
            limit: None,
            description: "托管云同步能力".into(),
        },
        EntitlementSummary {
            key: "version_history_days".into(),
            enabled: true,
            limit: plan.version_history_days.map(i64::from),
            description: "可查看和恢复的历史版本保留天数".into(),
        },
        EntitlementSummary {
            key: "cloud_devices".into(),
            enabled: true,
            limit: plan.max_devices.map(i64::from),
            description: "允许连接到托管云同步的设备数量".into(),
        },
        EntitlementSummary {
            key: "attachment_bytes".into(),
            enabled: true,
            limit: Some(plan.max_attachment_bytes),
            description: "托管附件总容量".into(),
        },
        EntitlementSummary {
            key: "priority_sync".into(),
            enabled: plan.sync_priority == "priority",
            limit: None,
            description: "同步优先级".into(),
        },
    ]
}

async fn usage_for_user(
    state: &AppState,
    user_id: Uuid,
    plan: &BillingPlan,
) -> Result<Vec<UsageMetric>, AppError> {
    let attachment_bytes = attachment_usage_bytes(state, user_id).await?;
    let device_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM sync_cursors
         WHERE user_id = $1 AND updated_at >= $2",
    )
    .bind(user_id)
    .bind(active_device_cutoff())
    .fetch_one(state.db.inner())
    .await?;
    Ok(vec![
        UsageMetric {
            key: "attachment_bytes".into(),
            used: attachment_bytes,
            limit: Some(plan.max_attachment_bytes),
            unit: "bytes".into(),
        },
        UsageMetric {
            key: "cloud_devices".into(),
            used: device_count,
            limit: plan.max_devices.map(i64::from),
            unit: "devices".into(),
        },
    ])
}

async fn persist_account_projection(
    state: &AppState,
    user_id: Uuid,
    entitlements: &[EntitlementSummary],
    usage: &[UsageMetric],
) -> Result<(), AppError> {
    let mut tx = state.db.inner().begin().await?;
    for entitlement in entitlements {
        sqlx::query(
            "INSERT INTO entitlements (user_id, key, enabled, limit_value, source, description, created_at, updated_at, created_by, updated_by)
             VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW(), NULL, NULL)
             ON CONFLICT (user_id, key)
             DO UPDATE SET enabled = EXCLUDED.enabled, limit_value = EXCLUDED.limit_value, source = EXCLUDED.source, description = EXCLUDED.description, updated_at = NOW(), updated_by = NULL",
        )
        .bind(user_id)
        .bind(&entitlement.key)
        .bind(entitlement.enabled)
        .bind(entitlement.limit)
        .bind("plan")
        .bind(&entitlement.description)
        .execute(&mut *tx)
        .await?;
    }
    for metric in usage {
        sqlx::query(
            "INSERT INTO usage_counters (user_id, key, used, created_at, updated_at, created_by, updated_by)
             VALUES ($1, $2, $3, NOW(), NOW(), NULL, NULL)
             ON CONFLICT (user_id, key)
             DO UPDATE SET used = EXCLUDED.used, updated_at = NOW(), updated_by = NULL",
        )
        .bind(user_id)
        .bind(&metric.key)
        .bind(metric.used)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

fn normalize_subscription_status(status: &str) -> String {
    match status {
        "active" | "trialing" | "past_due" | "paused" | "canceled" | "expired" => status,
        _ => "active",
    }
    .to_string()
}

fn billing_ready(state: &AppState) -> bool {
    matches!(
        state.config.billing_provider.as_deref(),
        Some("lemonsqueezy")
    ) && state.config.lemonsqueezy_api_key.is_some()
        && state.config.lemonsqueezy_store_id.is_some()
        && state.config.lemonsqueezy_webhook_secret.is_some()
        && state.config.lemonsqueezy_monthly_variant_id.is_some()
        && state.config.lemonsqueezy_yearly_variant_id.is_some()
        && !state.config.billing_public_origin.trim().is_empty()
}

async fn create_lemonsqueezy_checkout(
    state: &AppState,
    user: &User,
    price: &BillingPrice,
) -> Result<String, AppError> {
    if state.config.billing_provider.as_deref() != Some("lemonsqueezy") {
        return Err(AppError::BadRequest(
            "Billing provider is not configured for checkout".into(),
        ));
    }
    let api_key = state
        .config
        .lemonsqueezy_api_key
        .as_deref()
        .ok_or_else(|| AppError::Internal("LEMONSQUEEZY_API_KEY is not configured".into()))?;
    let store_id = state
        .config
        .lemonsqueezy_store_id
        .as_deref()
        .ok_or_else(|| AppError::Internal("LEMONSQUEEZY_STORE_ID is not configured".into()))?;
    let variant_id = variant_id_for_price(state, &price.id)?;
    let public_origin = state.config.billing_public_origin.trim_end_matches('/');
    let redirect_url = format!("{public_origin}/?checkout=success");
    let payload = json!({
        "data": {
            "type": "checkouts",
            "attributes": {
                "checkout_data": {
                    "email": user.email,
                    "custom": {
                        "user_id": user.id.to_string(),
                        "price_id": price.id,
                    }
                },
                "checkout_options": {
                    "embed": false,
                    "media": true,
                    "logo": true
                },
                "product_options": {
                    "redirect_url": redirect_url,
                    "receipt_button_text": "Return to QuickNote",
                    "receipt_link_url": public_origin
                }
            },
            "relationships": {
                "store": {
                    "data": {
                        "type": "stores",
                        "id": store_id
                    }
                },
                "variant": {
                    "data": {
                        "type": "variants",
                        "id": variant_id
                    }
                }
            }
        }
    });
    let response = state
        .http
        .post("https://api.lemonsqueezy.com/v1/checkouts")
        .header("Accept", "application/vnd.api+json")
        .header("Content-Type", "application/vnd.api+json")
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|error| AppError::Internal(format!("Checkout request failed: {error}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "Checkout request failed with {status}: {body}"
        )));
    }
    let payload: Value = response
        .json()
        .await
        .map_err(|error| AppError::Internal(format!("Invalid checkout response: {error}")))?;
    let checkout_url = payload
        .get("data")
        .and_then(|data| data.get("attributes"))
        .and_then(|attributes| attributes.get("url"))
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Internal("Checkout response did not include a URL".into()))?;
    Ok(checkout_url.to_string())
}

fn variant_id_for_price(state: &AppState, price_id: &str) -> Result<String, AppError> {
    match price_id {
        PRO_MONTHLY_PRICE_ID => state
            .config
            .lemonsqueezy_monthly_variant_id
            .clone()
            .ok_or_else(|| {
                AppError::Internal("LEMONSQUEEZY_MONTHLY_VARIANT_ID is not configured".into())
            }),
        PRO_YEARLY_PRICE_ID => state
            .config
            .lemonsqueezy_yearly_variant_id
            .clone()
            .ok_or_else(|| {
                AppError::Internal("LEMONSQUEEZY_YEARLY_VARIANT_ID is not configured".into())
            }),
        _ => Err(AppError::BadRequest("Unknown checkout price".into())),
    }
}

fn verify_lemonsqueezy_signature(
    state: &AppState,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(), AppError> {
    let secret = state
        .config
        .lemonsqueezy_webhook_secret
        .as_deref()
        .ok_or_else(|| {
            AppError::Internal("LEMONSQUEEZY_WEBHOOK_SECRET is not configured".into())
        })?;
    let signature = headers
        .get("X-Signature")
        .or_else(|| headers.get("x-signature"))
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::BadRequest("Missing X-Signature header".into()))?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|error| AppError::Internal(format!("Webhook verifier failed: {error}")))?;
    mac.update(body);
    let expected = hex_encode(mac.finalize().into_bytes());
    if !expected.eq_ignore_ascii_case(signature) {
        return Err(AppError::Auth);
    }
    Ok(())
}

async fn process_lemonsqueezy_event(
    state: &AppState,
    event_name: &str,
    payload: &LemonSqueezyWebhookPayload,
) -> Result<(), AppError> {
    if !event_name.starts_with("subscription_") {
        return Ok(());
    }
    let attributes = payload
        .data
        .get("attributes")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::BadRequest("Webhook payload is missing attributes".into()))?;
    let provider_subscription_id = payload
        .data
        .get("id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let custom = payload
        .meta
        .get("custom_data")
        .or_else(|| payload.meta.get("custom"))
        .and_then(Value::as_object);
    let user_id = custom
        .and_then(|item| item.get("user_id"))
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    let user_id = match (user_id, provider_subscription_id.as_deref()) {
        (Some(id), _) => Some(id),
        (None, Some(subscription_id)) => {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT user_id FROM subscriptions WHERE provider = $1 AND provider_subscription_id = $2",
            )
            .bind("lemonsqueezy")
            .bind(subscription_id)
            .fetch_optional(state.db.inner())
            .await?
        }
        _ => None,
    };
    let Some(user_id) = user_id else {
        return Ok(());
    };

    let price_id = subscription_price_id(state, attributes)?;
    let plan_id = if price_id.starts_with("pro-") {
        "pro"
    } else {
        FREE_PLAN_ID
    };
    let provider_customer_id = json_string(attributes.get("customer_id"));
    let status = json_string(attributes.get("status")).unwrap_or_else(|| "active".into());
    let cancel_at_period_end = attributes
        .get("cancelled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let current_period_start = parse_optional_datetime(
        attributes
            .get("current_period_start")
            .or_else(|| attributes.get("billing_anchor"))
            .or_else(|| attributes.get("updated_at")),
    );
    let current_period_end = parse_optional_datetime(
        attributes
            .get("renews_at")
            .or_else(|| attributes.get("ends_at")),
    );
    let trial_ends_at = parse_optional_datetime(attributes.get("trial_ends_at"));
    let canceled_at = parse_optional_datetime(attributes.get("ends_at"));
    let management_url = attributes
        .get("urls")
        .and_then(|value| value.get("customer_portal"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    sqlx::query(
        "INSERT INTO subscriptions
            (user_id, plan_id, price_id, provider, provider_customer_id, provider_subscription_id, status, cancel_at_period_end, current_period_start, current_period_end, trial_ends_at, canceled_at, management_url, created_at, updated_at, created_by, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW(), NOW(), NULL, NULL)
         ON CONFLICT (provider, provider_subscription_id)
         DO UPDATE SET
            plan_id = EXCLUDED.plan_id,
            price_id = EXCLUDED.price_id,
            provider_customer_id = EXCLUDED.provider_customer_id,
            status = EXCLUDED.status,
            cancel_at_period_end = EXCLUDED.cancel_at_period_end,
            current_period_start = EXCLUDED.current_period_start,
            current_period_end = EXCLUDED.current_period_end,
            trial_ends_at = EXCLUDED.trial_ends_at,
            canceled_at = EXCLUDED.canceled_at,
            management_url = EXCLUDED.management_url,
            updated_at = NOW(),
            updated_by = NULL",
    )
    .bind(user_id)
    .bind(plan_id)
    .bind(&price_id)
    .bind("lemonsqueezy")
    .bind(provider_customer_id)
    .bind(provider_subscription_id)
    .bind(status)
    .bind(cancel_at_period_end)
    .bind(current_period_start)
    .bind(current_period_end)
    .bind(trial_ends_at)
    .bind(canceled_at)
    .bind(management_url)
    .execute(state.db.inner())
    .await?;
    prune_versions_to_plan_window(state, user_id).await?;
    let plan = load_effective_plan(state, user_id).await?;
    let entitlements = entitlements_for_plan(&plan);
    let usage = usage_for_user(state, user_id, &plan).await?;
    persist_account_projection(state, user_id, &entitlements, &usage).await?;
    Ok(())
}

async fn ensure_user_exists(state: &AppState, user_id: Uuid) -> Result<(), AppError> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
        .bind(user_id)
        .fetch_one(state.db.inner())
        .await?;
    if exists {
        Ok(())
    } else {
        Err(AppError::Auth)
    }
}

async fn fetch_user_identity(state: &AppState, user_id: Uuid) -> Result<UserResponse, AppError> {
    let user: Option<(Uuid, String)> = sqlx::query_as("SELECT id, email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(state.db.inner())
        .await?;
    user.map(|(id, email)| UserResponse { id, email })
        .ok_or(AppError::Auth)
}

async fn load_effective_plan(state: &AppState, user_id: Uuid) -> Result<BillingPlan, AppError> {
    let plans = load_plans(state).await?;
    let subscription_records = fetch_subscription_records(state, user_id).await?;
    let effective_plan_id = effective_subscription(&subscription_records)
        .map(|item| item.plan_id.as_str())
        .unwrap_or(FREE_PLAN_ID);
    let free_plan = plans.iter().find(|item| item.id == FREE_PLAN_ID).cloned();
    plans
        .into_iter()
        .find(|item| item.id == effective_plan_id)
        .or(free_plan)
        .ok_or_else(|| AppError::Internal("Billing plans are not seeded".into()))
}

async fn load_plans(state: &AppState) -> Result<Vec<BillingPlan>, AppError> {
    sqlx::query_as(
        "SELECT id, name, tier, description, cloud_enabled, version_history_days, max_devices, max_attachment_bytes, sync_priority, checkout_cta
         FROM billing_plans
         ORDER BY CASE tier WHEN 'free' THEN 0 ELSE 1 END, name ASC",
    )
    .fetch_all(state.db.inner())
    .await
    .map_err(AppError::from)
}

async fn load_prices(state: &AppState) -> Result<Vec<BillingPrice>, AppError> {
    sqlx::query_as(
        "SELECT id, plan_id, provider, provider_price_id, billing_interval, currency, unit_amount, is_active
         FROM billing_prices
         WHERE is_active = true
         ORDER BY unit_amount ASC",
    )
    .fetch_all(state.db.inner())
    .await
    .map_err(AppError::from)
}

async fn fetch_subscription_records(
    state: &AppState,
    user_id: Uuid,
) -> Result<Vec<SubscriptionRecord>, AppError> {
    sqlx::query_as(
        "SELECT plan_id, price_id, provider, status, cancel_at_period_end, current_period_start, current_period_end, management_url
         FROM subscriptions
         WHERE user_id = $1
         ORDER BY
           CASE status
             WHEN 'active' THEN 0
             WHEN 'trialing' THEN 1
             WHEN 'past_due' THEN 2
             ELSE 3
           END,
           COALESCE(current_period_end, created_at) DESC,
           created_at DESC",
    )
    .bind(user_id)
    .fetch_all(state.db.inner())
    .await
    .map_err(AppError::from)
}

fn to_subscription_summary(item: &SubscriptionRecord) -> SubscriptionSummary {
    SubscriptionSummary {
        plan_id: item.plan_id.clone(),
        price_id: item.price_id.clone(),
        provider: item.provider.clone(),
        status: normalize_subscription_status(&item.status),
        cancel_at_period_end: item.cancel_at_period_end,
        current_period_start: item.current_period_start.map(format_datetime),
        current_period_end: item.current_period_end.map(format_datetime),
        management_url: item.management_url.clone(),
    }
}

fn effective_subscription(records: &[SubscriptionRecord]) -> Option<&SubscriptionRecord> {
    records.iter().find(|item| subscription_grants_access(item))
}

fn subscription_grants_access(record: &SubscriptionRecord) -> bool {
    match normalize_subscription_status(&record.status).as_str() {
        "active" | "trialing" | "past_due" => record
            .current_period_end
            .map(|value| value >= Utc::now())
            .unwrap_or(true),
        _ => false,
    }
}

async fn attachment_usage_bytes(state: &AppState, user_id: Uuid) -> Result<i64, AppError> {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(size), 0)::BIGINT FROM attachments WHERE user_id = $1",
    )
        .bind(user_id)
        .fetch_one(state.db.inner())
        .await
        .map_err(AppError::from)
}

fn active_device_cutoff() -> DateTime<Utc> {
    Utc::now() - chrono::Duration::days(ACTIVE_DEVICE_WINDOW_DAYS)
}

fn subscription_price_id(
    state: &AppState,
    attributes: &serde_json::Map<String, Value>,
) -> Result<String, AppError> {
    let variant_id = json_string(attributes.get("variant_id"));
    match variant_id.as_deref() {
        Some(id) if Some(id) == state.config.lemonsqueezy_monthly_variant_id.as_deref() => {
            Ok(PRO_MONTHLY_PRICE_ID.to_string())
        }
        Some(id) if Some(id) == state.config.lemonsqueezy_yearly_variant_id.as_deref() => {
            Ok(PRO_YEARLY_PRICE_ID.to_string())
        }
        _ => Err(AppError::BadRequest(
            "Webhook payload references an unknown billing variant".into(),
        )),
    }
}

fn json_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(ToOwned::to_owned)
}

fn parse_optional_datetime(value: Option<&Value>) -> Option<DateTime<Utc>> {
    value
        .and_then(Value::as_str)
        .and_then(|raw| chrono::DateTime::parse_from_rfc3339(raw).ok())
        .map(|value| value.with_timezone(&Utc))
}

fn format_datetime(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}
