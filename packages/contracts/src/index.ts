export interface Note {
  id: string;
  title: string;
  content: string;
  yjs_state?: number[] | null;
  yjs_state_version: number;
  is_pinned: boolean;
  sort_order: number;
  created_at: string;
  updated_at: string;
  version: number;
  is_deleted: boolean;
}

export interface NoteSummary {
  id: string;
  title: string;
  preview: string;
  is_pinned: boolean;
  created_at: string;
  updated_at: string;
}

export interface ClipboardItem {
  id: string;
  kind: "text" | "link" | "code" | "image" | "rich";
  content: string;
  preview: string;
  source_device: string;
  created_at: string;
  updated_at: string;
  last_copied_at: string;
  capture_count: number;
  is_pinned: boolean;
  is_deleted: boolean;
}

export interface AttachmentRecord {
  id: string;
  relative_path: string;
  mime_type: string;
  size: number;
  created_at: string;
}

export interface NoteVersion {
  id: number;
  note_id: string;
  title: string;
  content: string;
  version: number;
  created_at: string;
  is_pinned: boolean;
}

export type SaveStatus = "idle" | "saving" | "saved" | "retrying" | "error";
export type AppView = "notes" | "clipboard";

export interface AuthUser { id: string; email: string }
export interface AuthResponse { token: string; user: AuthUser }

export interface BillingPlan {
  id: string;
  name: string;
  tier: "free" | "pro";
  description: string;
  cloud_enabled: boolean;
  version_history_days: number | null;
  max_devices: number | null;
  max_attachment_bytes: number;
  sync_priority: "standard" | "priority";
  checkout_cta: string;
}

export interface BillingPrice {
  id: string;
  plan_id: string;
  provider: string;
  provider_price_id: string;
  billing_interval: "month" | "year";
  currency: string;
  unit_amount: number;
  is_active: boolean;
}

export interface SubscriptionSummary {
  plan_id: string;
  price_id: string;
  provider: string;
  status: "active" | "trialing" | "past_due" | "paused" | "canceled" | "expired";
  cancel_at_period_end: boolean;
  current_period_start: string | null;
  current_period_end: string | null;
  management_url: string | null;
}

export interface EntitlementSummary {
  key: string;
  enabled: boolean;
  limit: number | null;
  description: string;
}

export interface UsageMetric {
  key: string;
  used: number;
  limit: number | null;
  unit: string;
}

export interface AccountSummary {
  user: AuthUser;
  plans: BillingPlan[];
  prices: BillingPrice[];
  subscription: SubscriptionSummary | null;
  entitlements: EntitlementSummary[];
  usage: UsageMetric[];
  billing_provider: string | null;
  billing_ready: boolean;
}

export interface CreateCheckoutRequest {
  price_id: string;
}

export interface CheckoutSessionResponse {
  checkout_url: string;
}

export interface BillingPortalResponse {
  management_url: string;
}
