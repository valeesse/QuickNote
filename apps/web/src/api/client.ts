import type {
  AccountSummary,
  AuthResponse,
  BillingPortalResponse,
  CheckoutSessionResponse,
  CreateCheckoutRequest,
} from "@/types";

const USER_KEY = "quicknote-user";
const AUTH_EXPIRED_EVENT = "quicknote-auth-expired";

export function getBaseUrl(): string {
  return import.meta.env.VITE_API_BASE_URL || "";
}

export function getStoredUser(): AuthResponse["user"] | null {
  const raw = localStorage.getItem(USER_KEY);
  if (!raw) return null;
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

export function setAuth(_token: string, user: AuthResponse["user"]): void {
  localStorage.setItem(USER_KEY, JSON.stringify(user));
}

export function clearAuth(): void {
  localStorage.removeItem(USER_KEY);
}

export function subscribeToAuthExpiry(onExpire: () => void): () => void {
  const handler = () => onExpire();
  window.addEventListener(AUTH_EXPIRED_EVENT, handler);
  return () => window.removeEventListener(AUTH_EXPIRED_EVENT, handler);
}

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

function notifyAuthExpired(): void {
  clearAuth();
  window.dispatchEvent(new Event(AUTH_EXPIRED_EVENT));
}

function shouldNotifyAuthExpiry(path: string): boolean {
  return !path.startsWith("/api/auth/login") && !path.startsWith("/api/auth/register");
}

function buildFetchOptions(options: RequestInit): RequestInit {
  return {
    ...options,
    credentials: "include",
  };
}

export async function apiFetch<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const headers: Record<string, string> = {
    ...(options.headers as Record<string, string>),
  };
  if (options.body !== undefined && !headers["Content-Type"]) {
    headers["Content-Type"] = "application/json";
  }

  const res = await fetch(`${getBaseUrl()}${path}`, buildFetchOptions({
    ...options,
    headers,
  }));

  if (!res.ok) {
    if (res.status === 401 && shouldNotifyAuthExpiry(path)) notifyAuthExpired();
    let message = res.statusText;
    try {
      const body = await res.json();
      if (typeof body === "string") message = body;
      else if (body.error) message = body.error;
      else if (body.message) message = body.message;
    } catch {
      // ignore parse error
    }
    throw new ApiError(res.status, message);
  }

  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

async function authenticatedFetch(path: string, options: RequestInit): Promise<Response> {
  const headers = new Headers(options.headers);
  const response = await fetch(
    `${getBaseUrl()}${path}`,
    buildFetchOptions({ ...options, headers }),
  );
  if (!response.ok) {
    if (response.status === 401 && shouldNotifyAuthExpiry(path)) notifyAuthExpired();
    throw new ApiError(response.status, response.statusText);
  }
  return response;
}

// Auth API
export const authApi = {
  register: (email: string, password: string) =>
    apiFetch<AuthResponse>("/api/auth/register", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    }),
  login: (email: string, password: string) =>
    apiFetch<AuthResponse>("/api/auth/login", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    }),
  me: () => apiFetch<AuthResponse["user"]>("/api/auth/me"),
  refresh: () =>
    apiFetch<{ token: string }>("/api/auth/refresh", { method: "POST" }),
  logout: () =>
    apiFetch<{ ok: boolean }>("/api/auth/logout", { method: "POST" }),
};

// Notes API
export const notesApi = {
  list: (tag?: string | null) =>
    apiFetch<import("@/types").NoteSummary[]>(
      tag ? `/api/notes?tag=${encodeURIComponent(tag)}` : "/api/notes",
    ),
  get: (id: string) => apiFetch<import("@/types").Note>(`/api/notes/${id}`),
  tags: () => apiFetch<import("@/types").TagSummary[]>("/api/tags"),
  setTags: (id: string, tags: string[]) =>
    apiFetch<import("@/types").Note>(`/api/notes/${id}/tags`, {
      method: "PUT",
      body: JSON.stringify({ tags }),
    }),
  create: (content: string) =>
    apiFetch<import("@/types").Note>("/api/notes", {
      method: "POST",
      body: JSON.stringify({ content }),
    }),
  update: (id: string, content: string) =>
    apiFetch<import("@/types").Note>(`/api/notes/${id}`, {
      method: "PUT",
      body: JSON.stringify({ content }),
    }),
  delete: (id: string) =>
    apiFetch<boolean>(`/api/notes/${id}`, { method: "DELETE" }),
  restore: (id: string) =>
    apiFetch<boolean>(`/api/notes/${id}/restore`, { method: "POST" }),
  togglePin: (id: string) =>
    apiFetch<boolean>(`/api/notes/${id}/pin`, { method: "PATCH" }),
  reorder: (ids: string[], isPinned: boolean) =>
    apiFetch<boolean>("/api/notes/reorder", {
      method: "POST",
      body: JSON.stringify({ ids, is_pinned: isPinned }),
    }),
  search: (q: string) =>
    apiFetch<import("@/types").NoteSummary[]>(
      `/api/notes/search?q=${encodeURIComponent(q)}`,
    ),
  // Trash
  listDeleted: () =>
    apiFetch<import("@/types").NoteSummary[]>("/api/notes/trash"),
  purge: (id: string) =>
    apiFetch<boolean>(`/api/notes/${id}/purge`, { method: "DELETE" }),
  // Versions
  listVersions: (id: string) =>
    apiFetch<import("@/types").NoteVersion[]>(`/api/notes/${id}/versions`),
  restoreVersion: (noteId: string, versionId: number) =>
    apiFetch<import("@/types").Note>(
      `/api/notes/${noteId}/versions/${versionId}/restore`,
      { method: "POST" },
    ),
  toggleVersionPin: (versionId: number) =>
    apiFetch<boolean>(`/api/notes/versions/${versionId}/pin`, {
      method: "PATCH",
    }),
  deleteVersion: (versionId: number) =>
    apiFetch<boolean>(`/api/notes/versions/${versionId}`, {
      method: "DELETE",
    }),
  clearVersions: (noteId: string) =>
    apiFetch<boolean>(`/api/notes/${noteId}/versions`, { method: "DELETE" }),
};

// Clipboard API
export const clipboardApi = {
  list: () => apiFetch<import("@/types").ClipboardItem[]>("/api/clipboard"),
  capture: (content: string, kind?: string) =>
    apiFetch<import("@/types").ClipboardItem>("/api/clipboard", {
      method: "POST",
      body: JSON.stringify({ content, kind, source_device: "web" }),
    }),
  togglePin: (id: string) =>
    apiFetch<boolean>(`/api/clipboard/${id}/pin`, { method: "PATCH" }),
  delete: (id: string) =>
    apiFetch<boolean>(`/api/clipboard/${id}`, { method: "DELETE" }),
};

export const attachmentsApi = {
  upload: async (id: string, bytes: Uint8Array, mimeType: string) => {
    const response = await authenticatedFetch(`/api/attachments/${id}`, {
      method: "PUT",
      headers: { "Content-Type": mimeType },
      body: bytes as BodyInit,
    });
    return response.json() as Promise<import("@/types").AttachmentRecord>;
  },
  download: async (id: string) => {
    const response = await authenticatedFetch(`/api/attachments/${id}`, { method: "GET" });
    return response.blob();
  },
};

export const billingApi = {
  summary: () => apiFetch<AccountSummary>("/api/account/summary"),
  createCheckout: (payload: CreateCheckoutRequest) =>
    apiFetch<CheckoutSessionResponse>("/api/billing/checkout", {
      method: "POST",
      body: JSON.stringify(payload),
    }),
  portal: () =>
    apiFetch<BillingPortalResponse>("/api/billing/portal", {
      method: "POST",
    }),
};
