import type { Page } from "@playwright/test";

export const noteItem = (page: Page, title: string) =>
  page.locator("[data-note-id]").filter({ hasText: title }).first();

export async function installMockBackend(page: Page): Promise<void> {
  await page.addInitScript(() => {
    type Note = {
      id: string;
      title: string;
      content: string;
      yjs_state?: number[] | null;
      yjs_state_version?: number;
      is_pinned: boolean;
      created_at: string;
      updated_at: string;
      version: number;
      is_deleted: boolean;
      tags: string[];
    };

    const key = "quicknote-e2e-db";
    const clipboardKey = "quicknote-e2e-clipboard-db";
    const load = (): Note[] => JSON.parse(localStorage.getItem(key) || "[]");
    const save = (notes: Note[]) => localStorage.setItem(key, JSON.stringify(notes));
    const text = (html: string) => html.replace(/<img\b[^>]*>/gi, " [图片] ").replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim();
    const title = (html: string) => text(html).slice(0, 100) || "无标题";
    const summary = (note: Note) => ({
      id: note.id,
      title: note.title,
      preview: text(note.content).slice(0, 200),
      is_pinned: note.is_pinned,
      created_at: note.created_at,
      updated_at: note.updated_at,
      tags: note.tags,
    });
    const loadClipboard = (): any[] => JSON.parse(localStorage.getItem(clipboardKey) || "[]");
    const saveClipboard = (items: any[]) => localStorage.setItem(clipboardKey, JSON.stringify(items));

    (window as any).isTauri = true;
    (window as any).__TAURI_INTERNALS__ = {
      convertFileSrc: (path: string) => `asset://localhost/${path.replaceAll("\\", "/")}`,
      invoke: async (cmd: string, args: any = {}) => {
        const notes = load();
        if (cmd === "create_note") {
          const now = new Date().toISOString();
          const note: Note = {
            id: crypto.randomUUID(),
            title: title(args.content || ""),
            content: args.content || "",
            yjs_state: null,
            yjs_state_version: 0,
            is_pinned: false,
            created_at: now,
            updated_at: now,
            version: 1,
            is_deleted: false,
            tags: [],
          };
          save([note, ...notes]);
          return note;
        }
        if (cmd === "list_notes") return notes.filter((note) => !note.is_deleted).map(summary);
        if (cmd === "list_notes_by_tag") {
          const tag = String(args.tag || "").toLowerCase();
          return notes
            .filter((note) => !note.is_deleted && note.tags.some((item) => item.toLowerCase() === tag))
            .map(summary);
        }
        if (cmd === "list_tags") {
          const counts = new Map<string, { name: string; count: number }>();
          for (const note of notes.filter((item) => !item.is_deleted)) {
            for (const tag of note.tags) {
              const normalized = tag.toLowerCase();
              const current = counts.get(normalized) || { name: tag, count: 0 };
              current.count += 1;
              counts.set(normalized, current);
            }
          }
          return [...counts.entries()].map(([normalized_name, item]) => ({
            id: `tag-${normalized_name}`,
            name: item.name,
            normalized_name,
            color: null,
            note_count: item.count,
          }));
        }
        if (cmd === "get_note") return notes.find((note) => note.id === args.id && !note.is_deleted) || null;
        if (cmd === "set_note_tags") {
          const note = notes.find((item) => item.id === args.noteId && !item.is_deleted);
          if (!note) return null;
          note.tags = [...new Set((args.tags || []).map((tag: string) => tag.trim()).filter(Boolean))];
          save(notes);
          return note;
        }
        if (cmd === "update_note") {
          const delay = Number(localStorage.getItem("quicknote-e2e-save-delay") || 0);
          if (delay) await new Promise((resolve) => setTimeout(resolve, delay));
          const failures = Number(localStorage.getItem("quicknote-e2e-save-failures") || 0);
          if (failures > 0) {
            localStorage.setItem("quicknote-e2e-save-failures", String(failures - 1));
            throw new Error("simulated save failure");
          }
          const note = notes.find((item) => item.id === args.id);
          if (!note) return null;
          note.content = args.content;
          note.title = title(args.content);
          if (Array.isArray(args.yjsState)) {
            note.yjs_state = args.yjsState;
            note.yjs_state_version = (note.yjs_state_version || 0) + 1;
          }
          note.updated_at = new Date().toISOString();
          note.version += 1;
          save(notes);
          return note;
        }
        if (cmd === "search_notes") {
          const query = String(args.query || "").toLowerCase();
          return notes
            .filter((note) => !note.is_deleted && `${note.title} ${text(note.content)}`.toLowerCase().includes(query))
            .map(summary);
        }
        if (cmd === "save_attachment") {
          const id = crypto.randomUUID();
          return { id, path: `C:\\QuickNote\\attachments\\${id}.webp` };
        }
        if (cmd === "get_attachment") {
          return { id: args.id, path: `C:\\QuickNote\\attachments\\${args.id}.webp` };
        }
        if (cmd === "cleanup_attachments") return 0;
        if (cmd === "clipboard_auto_capture_supported") return true;
        if (cmd === "set_clipboard_auto_capture_enabled") return args?.enabled ?? true;
        if (cmd === "prime_clipboard_capture") return true;
        if (cmd === "capture_clipboard") {
          const content = localStorage.getItem("quicknote-e2e-system-clipboard") || "";
          if (!content.trim()) return null;
          const items = loadClipboard();
          let item = items.find((entry) => entry.content === content);
          const now = new Date().toISOString();
          if (item) {
            item.capture_count += 1;
            item.last_copied_at = now;
            item.is_deleted = false;
          } else {
            item = {
              id: crypto.randomUUID(),
              kind: /^https?:\/\/\S+$/.test(content) ? "link" : content.includes("\n") && content.includes("{") ? "code" : "text",
              content,
              preview: content.slice(0, 240),
              source_device: "e2e-device",
              created_at: now,
              updated_at: now,
              last_copied_at: now,
              capture_count: 1,
              is_pinned: false,
              is_deleted: false,
            };
            items.push(item);
          }
          saveClipboard(items);
          return item;
        }
        if (cmd === "list_clipboard_items") {
          const query = String(args.query || "").toLowerCase();
          return loadClipboard()
            .filter((item) => !item.is_deleted && item.content.toLowerCase().includes(query))
            .sort((a, b) => Number(b.is_pinned) - Number(a.is_pinned) || b.last_copied_at.localeCompare(a.last_copied_at));
        }
        if (cmd === "copy_clipboard_item") {
          const items = loadClipboard();
          const item = items.find((entry) => entry.id === args.id && !entry.is_deleted);
          if (!item) return false;
          localStorage.setItem("quicknote-e2e-system-clipboard", item.content);
          item.last_copied_at = new Date().toISOString();
          saveClipboard(items);
          return true;
        }
        if (cmd === "toggle_clipboard_pin") {
          const items = loadClipboard();
          const item = items.find((entry) => entry.id === args.id);
          if (item) item.is_pinned = !item.is_pinned;
          saveClipboard(items);
          return Boolean(item);
        }
        if (cmd === "delete_clipboard_item") {
          const items = loadClipboard();
          const index = items.findIndex((entry) => entry.id === args.id);
          if (index !== -1) items.splice(index, 1);
          saveClipboard(items);
          return index !== -1;
        }
        if (cmd === "get_sync_config") {
          return {
            enabled: localStorage.getItem("quicknote-e2e-sync-enabled") === "true",
            provider: "webdav",
            endpoint: "https://dav.example.test/quicknote",
            username: "tester",
            device_id: "e2e-device",
          };
        }
        if (cmd === "set_sync_config") return { ...args.config, device_id: "e2e-device" };
        if (cmd === "has_pending_sync_changes" || cmd === "has_sync_changes") {
          localStorage.setItem(
            "quicknote-e2e-sync-check-count",
            String(Number(localStorage.getItem("quicknote-e2e-sync-check-count") || 0) + 1),
          );
          return localStorage.getItem("quicknote-e2e-sync-dirty") === "true"
            || localStorage.getItem("quicknote-e2e-sync-remote-dirty") === "true";
        }
        if (cmd === "pending_sync_change_count") {
          return localStorage.getItem("quicknote-e2e-sync-dirty") === "true" ? 1 : 0;
        }
        if (cmd === "sync_now") {
          localStorage.setItem(
            "quicknote-e2e-sync-count",
            String(Number(localStorage.getItem("quicknote-e2e-sync-count") || 0) + 1),
          );
          const delay = Number(localStorage.getItem("quicknote-e2e-sync-delay") || 0);
          if (delay) await new Promise((resolve) => setTimeout(resolve, delay));
          const remoteContent = localStorage.getItem("quicknote-e2e-remote-content");
          const note = notes.find((item) => !item.is_deleted);
          if (remoteContent && note) {
            note.content = remoteContent;
            note.title = title(remoteContent);
            note.updated_at = new Date().toISOString();
            note.version += 1;
            save(notes);
          }
          localStorage.setItem("quicknote-e2e-sync-dirty", "false");
          localStorage.setItem("quicknote-e2e-sync-remote-dirty", "false");
          return { pushed: 0, pulled: remoteContent && note ? 1 : 0, conflicts: 0 };
        }
        if (cmd.includes("plugin:event|listen")) return 1;
        if (cmd.includes("plugin:event|unlisten")) return null;
        if (cmd === "list_deleted_notes") return notes.filter((note) => note.is_deleted).map(summary);
        if (cmd === "delete_note") {
          const note = notes.find((item) => item.id === args.id);
          if (note) note.is_deleted = true;
          save(notes);
          return Boolean(note);
        }
        if (cmd === "restore_note") {
          const note = notes.find((item) => item.id === args.id);
          if (note) note.is_deleted = false;
          save(notes);
          return Boolean(note);
        }
        if (cmd === "purge_note") {
          save(notes.filter((item) => item.id !== args.id));
          return true;
        }
        if (cmd === "toggle_pin") return true;
        if (cmd === "get_note_versions") return [];
        if (cmd === "restore_note_version") return null;
        throw new Error(`Unhandled command: ${cmd}`);
      },
    };
  });
  await page.goto("/");
}
