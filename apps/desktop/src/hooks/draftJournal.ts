export interface DraftState {
  content: string;
  yjsState?: number[];
  revision: number;
  persistedRevision: number;
  timer: ReturnType<typeof setTimeout> | null;
  retryTimer: ReturnType<typeof setTimeout> | null;
  inFlight: Promise<boolean> | null;
}

interface DraftJournal {
  schemaVersion: 1;
  drafts: Record<string, { content: string; updatedAt: string }>;
}

const DRAFT_JOURNAL_KEY = "quicknote-draft-journal-v1";

export function loadDraftJournal(): Map<string, DraftState> {
  const drafts = new Map<string, DraftState>();
  if (typeof window === "undefined") return drafts;

  try {
    const raw = window.localStorage.getItem(DRAFT_JOURNAL_KEY);
    if (!raw) return drafts;
    const journal = JSON.parse(raw) as Partial<DraftJournal>;
    if (journal.schemaVersion !== 1 || !journal.drafts || typeof journal.drafts !== "object") {
      return drafts;
    }
    for (const [id, entry] of Object.entries(journal.drafts)) {
      if (!id || !entry || typeof entry.content !== "string") continue;
      drafts.set(id, {
        content: entry.content,
        revision: 1,
        persistedRevision: 0,
        timer: null,
        retryTimer: null,
        inFlight: null,
      });
    }
  } catch (err) {
    console.error("Failed to read draft journal:", err);
  }
  return drafts;
}

export function persistDraftJournal(drafts: Map<string, DraftState>): void {
  if (typeof window === "undefined") return;
  const entries: DraftJournal["drafts"] = {};
  const updatedAt = new Date().toISOString();
  for (const [id, draft] of drafts) {
    if (draft.persistedRevision >= draft.revision) continue;
    entries[id] = { content: draft.content, updatedAt };
  }
  if (Object.keys(entries).length === 0) {
    window.localStorage.removeItem(DRAFT_JOURNAL_KEY);
    return;
  }
  const journal: DraftJournal = { schemaVersion: 1, drafts: entries };
  window.localStorage.setItem(DRAFT_JOURNAL_KEY, JSON.stringify(journal));
}

export function getErrorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function createSummaryFromContent(content: string): { title: string; preview: string } {
  const text = htmlToPlainText(content);
  const lines = text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);

  return {
    title: lines[0]?.slice(0, 100) || "无标题",
    preview: lines.slice(1).join(" ").slice(0, 200),
  };
}

function htmlToPlainText(content: string): string {
  if (typeof DOMParser === "undefined") {
    return content.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim();
  }

  const doc = new DOMParser().parseFromString(`<main>${content}</main>`, "text/html");
  const blocks = Array.from(doc.querySelectorAll("p,h1,h2,h3,h4,h5,h6,li,blockquote"));
  const lines = blocks
    .map((node) => node.textContent?.replace(/\s+/g, " ").trim() ?? "")
    .filter(Boolean);

  if (lines.length > 0) {
    return lines.join("\n");
  }

  return doc.body.textContent?.replace(/\s+/g, " ").trim() ?? "";
}
