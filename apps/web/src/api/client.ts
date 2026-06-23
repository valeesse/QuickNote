import type { AuthResponse } from "@/types";

const TOKEN_KEY = "quicknote-token";
const USER_KEY = "quicknote-user";

function getBaseUrl(): string {
  return import.meta.env.VITE_API_BASE_URL || "";
}

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
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

export function setAuth(token: string, user: AuthResponse["user"]): void {
  localStorage.setItem(TOKEN_KEY, token);
  localStorage.setItem(USER_KEY, JSON.stringify(user));
}

export function clearAuth(): void {
  localStorage.removeItem(TOKEN_KEY);
  localStorage.removeItem(USER_KEY);
}

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

export async function apiFetch<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const token = getToken();
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(options.headers as Record<string, string>),
  };
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  const res = await fetch(`${getBaseUrl()}${path}`, {
    ...options,
    headers,
  });

  if (!res.ok) {
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
  const token = getToken();
  const headers = new Headers(options.headers);
  if (token) headers.set("Authorization", `Bearer ${token}`);
  const response = await fetch(`${getBaseUrl()}${path}`, { ...options, headers });
  if (!response.ok) throw new ApiError(response.status, response.statusText);
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
  refresh: () =>
    apiFetch<{ token: string }>("/api/auth/refresh", { method: "POST" }),
};

// Notes API
export const notesApi = {
  list: () => apiFetch<import("@/types").NoteSummary[]>("/api/notes"),
  get: (id: string) => apiFetch<import("@/types").Note>(`/api/notes/${id}`),
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
