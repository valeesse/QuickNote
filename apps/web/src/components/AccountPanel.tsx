import { useMemo, useState } from "react";
import { X, CreditCard, Loader2, Sparkles, ShieldCheck } from "lucide-react";
import type { AccountSummary, BillingPrice, BillingPlan } from "@/types";

interface AccountPanelProps {
  summary: AccountSummary | null;
  loading: boolean;
  error: string | null;
  onClose: () => void;
  onRefresh: () => void | Promise<void>;
  onCheckout: (priceId: string) => Promise<void>;
  onManageBilling: () => Promise<void>;
}

export function AccountPanel({
  summary,
  loading,
  error,
  onClose,
  onRefresh,
  onCheckout,
  onManageBilling,
}: AccountPanelProps) {
  const [busyPriceId, setBusyPriceId] = useState<string | null>(null);
  const [managing, setManaging] = useState(false);

  const groupedPrices = useMemo(() => {
    const buckets = new Map<string, BillingPrice[]>();
    for (const price of summary?.prices ?? []) {
      const bucket = buckets.get(price.plan_id) ?? [];
      bucket.push(price);
      buckets.set(price.plan_id, bucket);
    }
    for (const bucket of buckets.values()) {
      bucket.sort((left, right) => left.unit_amount - right.unit_amount);
    }
    return buckets;
  }, [summary?.prices]);

  const usageByKey = useMemo(
    () => new Map((summary?.usage ?? []).map((item) => [item.key, item])),
    [summary?.usage],
  );

  return (
    <div className="fixed inset-0 z-50 flex justify-end bg-slate-950/30 backdrop-blur-sm">
      <div className="flex h-full w-full max-w-xl flex-col bg-white shadow-2xl">
        <div className="flex items-start justify-between border-b border-gray-100 px-6 py-5">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.24em] text-emerald-600">
              Cloud Account
            </p>
            <h2 className="mt-2 text-2xl font-semibold text-gray-900">账户与订阅</h2>
            <p className="mt-1 text-sm text-gray-500">
              WebDAV 永久免费，云同步按账户套餐提供托管能力。
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="focus-ring rounded-full p-2 text-gray-400 hover:bg-gray-100 hover:text-gray-700"
            aria-label="关闭账户面板"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        <div className="flex-1 space-y-6 overflow-y-auto px-6 py-6">
          <section className="rounded-2xl border border-emerald-100 bg-emerald-50/70 p-5">
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-sm font-medium text-emerald-900">
                  {summary?.user.email ?? "正在加载账户..."}
                </p>
                <p className="mt-1 text-sm text-emerald-700">
                  当前套餐：
                  <span className="ml-1 font-semibold">
                    {summary?.subscription?.plan_id === "pro" ? "Cloud Pro" : "Cloud Free"}
                  </span>
                </p>
                {summary?.subscription?.current_period_end && (
                  <p className="mt-1 text-xs text-emerald-700/80">
                    周期结束：{formatDate(summary.subscription.current_period_end)}
                    {summary.subscription.cancel_at_period_end ? " · 已设为到期取消" : ""}
                  </p>
                )}
              </div>
              <button
                type="button"
                onClick={() => void onRefresh()}
                className="focus-ring rounded-full border border-emerald-200 bg-white px-3 py-1.5 text-xs font-medium text-emerald-700 hover:bg-emerald-100"
              >
                刷新状态
              </button>
            </div>

            {summary?.subscription?.plan_id === "pro" && (
              <button
                type="button"
                onClick={() => {
                  setManaging(true);
                  void onManageBilling().finally(() => setManaging(false));
                }}
                className="focus-ring mt-4 inline-flex items-center gap-2 rounded-xl bg-emerald-900 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-800"
              >
                {managing ? <Loader2 className="h-4 w-4 animate-spin" /> : <CreditCard className="h-4 w-4" />}
                管理订阅
              </button>
            )}
          </section>

          <section>
            <div className="mb-3 flex items-center gap-2">
              <ShieldCheck className="h-4 w-4 text-slate-500" />
              <h3 className="text-sm font-semibold text-slate-900">用量与限制</h3>
            </div>
            <div className="grid gap-3 sm:grid-cols-2">
              {(summary?.usage ?? []).map((metric) => (
                <div key={metric.key} className="rounded-2xl border border-gray-200 bg-white p-4">
                  <p className="text-xs font-medium uppercase tracking-wide text-gray-500">
                    {metricLabel(metric.key)}
                  </p>
                  <p className="mt-2 text-lg font-semibold text-gray-900">
                    {formatUsage(metric.used, metric.unit)}
                    {metric.limit !== null ? (
                      <span className="ml-1 text-sm font-medium text-gray-400">
                        / {formatUsage(metric.limit, metric.unit)}
                      </span>
                    ) : (
                      <span className="ml-1 text-sm font-medium text-emerald-600">无限制</span>
                    )}
                  </p>
                  {metric.limit !== null && (
                    <div className="mt-3 h-2 overflow-hidden rounded-full bg-gray-100">
                      <div
                        className="h-full rounded-full bg-emerald-500"
                        style={{ width: `${Math.min((metric.used / metric.limit) * 100, 100)}%` }}
                      />
                    </div>
                  )}
                </div>
              ))}
            </div>
          </section>

          <section>
            <div className="mb-3 flex items-center gap-2">
              <Sparkles className="h-4 w-4 text-amber-500" />
              <h3 className="text-sm font-semibold text-slate-900">套餐与升级</h3>
            </div>
            <div className="space-y-4">
              {(summary?.plans ?? []).map((plan) => (
                <PlanCard
                  key={plan.id}
                  plan={plan}
                  prices={groupedPrices.get(plan.id) ?? []}
                  currentPlanId={summary?.subscription?.plan_id ?? "free"}
                  entitlements={summary?.entitlements ?? []}
                  usageByKey={usageByKey}
                  billingReady={summary?.billing_ready ?? false}
                  busyPriceId={busyPriceId}
                  onCheckout={async (priceId) => {
                    setBusyPriceId(priceId);
                    try {
                      await onCheckout(priceId);
                    } finally {
                      setBusyPriceId(null);
                    }
                  }}
                />
              ))}
            </div>
          </section>

          {error && (
            <div className="rounded-2xl border border-rose-200 bg-rose-50 px-4 py-3 text-sm text-rose-700">
              {error}
            </div>
          )}
          {loading && (
            <div className="rounded-2xl border border-gray-200 bg-gray-50 px-4 py-3 text-sm text-gray-500">
              正在同步账户信息...
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function PlanCard({
  plan,
  prices,
  currentPlanId,
  entitlements,
  usageByKey,
  billingReady,
  busyPriceId,
  onCheckout,
}: {
  plan: BillingPlan;
  prices: BillingPrice[];
  currentPlanId: string;
  entitlements: AccountSummary["entitlements"];
  usageByKey: Map<string, AccountSummary["usage"][number]>;
  billingReady: boolean;
  busyPriceId: string | null;
  onCheckout: (priceId: string) => Promise<void>;
}) {
  const isCurrent = currentPlanId === plan.id;
  const attachmentUsage = usageByKey.get("attachment_bytes");
  const deviceUsage = usageByKey.get("cloud_devices");
  const [selectedInterval, setSelectedInterval] = useState<"month" | "year">("year");
  const featureItems = [
    {
      label: "版本历史",
      value: plan.version_history_days === null ? "无限制" : `${plan.version_history_days} 天`,
    },
    {
      label: "设备数",
      value: plan.max_devices === null ? "无限制" : `${plan.max_devices} 台`,
    },
    {
      label: "附件容量",
      value: formatBytes(plan.max_attachment_bytes),
    },
    {
      label: "同步优先级",
      value: plan.sync_priority === "priority" ? "优先队列" : "标准队列",
    },
  ];
  const monthlyPrice = prices.find((item) => item.billing_interval === "month");
  const yearlyPrice = prices.find((item) => item.billing_interval === "year");
  const selectedPrice =
    selectedInterval === "year" ? (yearlyPrice ?? monthlyPrice) : (monthlyPrice ?? yearlyPrice);
  const savings =
    monthlyPrice && yearlyPrice
      ? Math.max(monthlyPrice.unit_amount * 12 - yearlyPrice.unit_amount, 0)
      : 0;

  return (
    <div
      className={`overflow-hidden rounded-3xl border p-5 ${
        plan.tier === "pro"
          ? "border-emerald-300 bg-[radial-gradient(circle_at_top_left,_rgba(16,185,129,0.16),_transparent_34%),linear-gradient(135deg,#f3fff8_0%,#ffffff_72%)] shadow-[0_18px_48px_-28px_rgba(16,185,129,0.45)]"
          : "border-slate-200 bg-[linear-gradient(180deg,#ffffff_0%,#fbfcfe_100%)]"
      }`}
    >
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-3">
            <div
              className={`inline-flex rounded-full px-3 py-1 text-[11px] font-semibold uppercase tracking-[0.18em] ${
                plan.tier === "pro" ? "bg-emerald-900 text-white" : "bg-slate-900 text-white"
              }`}
            >
              {plan.tier}
            </div>
            {plan.tier === "pro" && (
              <span className="rounded-full border border-amber-200 bg-amber-50 px-2.5 py-1 text-[11px] font-semibold text-amber-700">
                推荐
              </span>
            )}
          </div>
          <div className="mt-4 flex flex-wrap items-end gap-x-3 gap-y-2">
            <h4 className="text-2xl font-semibold tracking-tight text-slate-900">{plan.name}</h4>
            {!isCurrent && selectedPrice && (
              <p className="text-sm font-medium text-slate-500">
                {selectedPrice.billing_interval === "year"
                  ? `${formatPrice(selectedPrice)} / 年`
                  : `${formatPrice(selectedPrice)} / 月`}
              </p>
            )}
          </div>
          <p className="mt-2 max-w-md text-sm leading-6 text-slate-600">{plan.description}</p>
        </div>
        {isCurrent && (
          <span className="shrink-0 rounded-full bg-emerald-100 px-3 py-1.5 text-xs font-semibold text-emerald-700">
            当前使用中
          </span>
        )}
      </div>

      <div className="mt-5 grid gap-3 sm:grid-cols-2">
        {featureItems.map((item) => (
          <div
            key={item.label}
            className={`rounded-2xl border px-4 py-3 ${
              plan.tier === "pro" ? "border-emerald-100 bg-white/85" : "border-slate-200 bg-slate-50/60"
            }`}
          >
            <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-slate-400">
              {item.label}
            </p>
            <p className="mt-1 text-sm font-semibold text-slate-800">{item.value}</p>
          </div>
        ))}
      </div>

      {plan.id === "free" && attachmentUsage && deviceUsage && (
        <div className="mt-4 rounded-2xl border border-slate-200 bg-slate-50 px-4 py-3 text-xs text-slate-600">
          当前使用：{formatBytes(attachmentUsage.used)} / {formatBytes(plan.max_attachment_bytes)}，
          设备 {deviceUsage.used}/{plan.max_devices ?? deviceUsage.used}
        </div>
      )}

      {plan.tier === "pro" && (monthlyPrice || yearlyPrice) && (
        <div className="mt-5 flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-emerald-100 bg-white/80 px-3 py-3">
          <div className="inline-flex rounded-2xl bg-slate-100 p-1">
            {monthlyPrice && (
              <button
                type="button"
                onClick={() => setSelectedInterval("month")}
                className={`rounded-xl px-3 py-2 text-sm font-medium transition ${
                  selectedInterval === "month"
                    ? "bg-white text-slate-900 shadow-sm"
                    : "text-slate-500 hover:text-slate-700"
                }`}
              >
                月付
              </button>
            )}
            {yearlyPrice && (
              <button
                type="button"
                onClick={() => setSelectedInterval("year")}
                className={`rounded-xl px-3 py-2 text-sm font-medium transition ${
                  selectedInterval === "year"
                    ? "bg-white text-slate-900 shadow-sm"
                    : "text-slate-500 hover:text-slate-700"
                }`}
              >
                年付
              </button>
            )}
          </div>
          <div className="text-right">
            <p className="text-sm font-semibold text-slate-900">
              {selectedPrice ? formatPrice(selectedPrice) : "即将开放"}
              {selectedPrice && (
                <span className="ml-1 text-xs font-medium text-slate-500">
                  / {selectedPrice.billing_interval === "year" ? "年" : "月"}
                </span>
              )}
            </p>
            {savings > 0 && selectedInterval === "year" && (
              <p className="text-xs font-medium text-emerald-700">
                年付立省 {formatCurrencyAmount(savings, yearlyPrice?.currency ?? "USD")}
              </p>
            )}
          </div>
        </div>
      )}

      <div className="mt-5 flex flex-wrap gap-3">
        {prices.length === 0 ? (
          <button
            type="button"
            disabled
            className="rounded-xl border border-gray-200 bg-gray-50 px-4 py-2 text-sm font-medium text-gray-400"
          >
            {isCurrent ? plan.checkout_cta : "当前不可购买"}
          </button>
        ) : (
          (plan.tier === "pro" && selectedPrice ? [selectedPrice] : prices).map((price) => (
            <button
              key={price.id}
              type="button"
              disabled={isCurrent || !billingReady || busyPriceId === price.id}
              onClick={() => void onCheckout(price.id)}
              className={`focus-ring inline-flex items-center gap-2 rounded-2xl px-4 py-2.5 text-sm font-medium transition ${
                isCurrent
                  ? "border border-gray-200 bg-gray-100 text-gray-400"
                  : plan.tier === "pro"
                    ? "bg-emerald-600 text-white hover:bg-emerald-700 disabled:cursor-not-allowed disabled:bg-gray-300"
                    : "bg-slate-900 text-white hover:bg-slate-800 disabled:cursor-not-allowed disabled:bg-gray-300"
              }`}
            >
              {busyPriceId === price.id && <Loader2 className="h-4 w-4 animate-spin" />}
              {plan.tier === "pro"
                ? `立即开通 ${price.billing_interval === "year" ? "年付" : "月付"}`
                : isCurrent
                  ? plan.checkout_cta
                  : "选择套餐"}
            </button>
          ))
        )}
      </div>

      {!billingReady && plan.tier === "pro" && (
        <p className="mt-3 text-xs text-amber-700">
          收费通道还未配置完成，先保留升级入口和权限模型。
        </p>
      )}
      {entitlements.length === 0 && (
        <p className="mt-3 text-xs text-slate-500">账户权益将在登录后自动同步。</p>
      )}
    </div>
  );
}

function metricLabel(key: string): string {
  if (key === "attachment_bytes") return "附件空间";
  if (key === "cloud_devices") return "云同步设备";
  return key;
}

function formatUsage(value: number, unit: string): string {
  return unit === "bytes" ? formatBytes(value) : `${value}`;
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KB`;
  if (value < 1024 ** 3) return `${(value / 1024 ** 2).toFixed(1)} MB`;
  return `${(value / 1024 ** 3).toFixed(1)} GB`;
}

function formatPrice(price: BillingPrice): string {
  return formatCurrencyAmount(price.unit_amount, price.currency);
}

function formatCurrencyAmount(amountInMinor: number, currency: string): string {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency,
    minimumFractionDigits: 0,
    maximumFractionDigits: 2,
  }).format(amountInMinor / 100);
}

function formatDate(value: string): string {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return parsed.toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}
