import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    type Note = {
      id: string;
      title: string;
      content: string;
      is_pinned: boolean;
      created_at: string;
      updated_at: string;
      version: number;
      is_deleted: boolean;
    };

    const key = "quicknote-e2e-db";
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
    });

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
            is_pinned: false,
            created_at: now,
            updated_at: now,
            version: 1,
            is_deleted: false,
          };
          save([note, ...notes]);
          return note;
        }
        if (cmd === "list_notes") return notes.filter((note) => !note.is_deleted).map(summary);
        if (cmd === "get_note") return notes.find((note) => note.id === args.id && !note.is_deleted) || null;
        if (cmd === "update_note") {
          const note = notes.find((item) => item.id === args.id);
          if (!note) return null;
          note.content = args.content;
          note.title = title(args.content);
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
        if (cmd === "save_attachment") return { path: `C:\\QuickNote\\attachments\\${crypto.randomUUID()}.webp` };
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
});

test("creates, edits, inserts an image, searches, and restores after reload", async ({ page }) => {
  await page.getByTitle("新建便签 (Ctrl+N)").click();
  const editor = page.locator(".tiptap").first();
  await expect(editor).toBeVisible();

  await editor.click();
  await page.keyboard.type("中文搜索便签");
  await expect(page.getByText("已保存")).toBeVisible();

  const chooser = page.waitForEvent("filechooser");
  await page.getByTitle("插入图片").click();
  const fileChooser = await chooser;
  await fileChooser.setFiles({
    name: "pixel.png",
    mimeType: "image/png",
    buffer: Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=",
      "base64"
    ),
  });
  await expect(page.locator(".tiptap img")).toHaveCount(1);

  await page.reload();
  await expect(page.getByRole("heading", { name: "中文搜索便签" })).toBeVisible();

  await page.getByPlaceholder("搜索便签...").fill("中文搜索");
  await expect(page.getByRole("heading", { name: "中文搜索便签" })).toBeVisible();
});

test("imports and exports markdown", async ({ page }) => {
  await page.getByTitle("新建便签 (Ctrl+N)").click();

  const chooser = page.waitForEvent("filechooser");
  await page.getByTitle("导入 Markdown").click();
  const fileChooser = await chooser;
  await fileChooser.setFiles({
    name: "note.md",
    mimeType: "text/markdown",
    buffer: Buffer.from("# Markdown 标题\n\n- 任务一", "utf-8"),
  });

  await expect(page.locator(".tiptap h1")).toHaveText("Markdown 标题");
  await page.getByTitle("Markdown 源码").click();
  await expect(page.locator("textarea")).toContainText("# Markdown 标题");
  await page.locator("textarea").fill("# 新标题\n\n- 任务二");
  await page.getByTitle("预览").click();
  await expect(page.locator(".tiptap h1")).toHaveText("新标题");
  await expect(page.getByTitle("复制为 Markdown")).toBeVisible();
  await expect(page.getByTitle("导出 Markdown")).toBeVisible();
});
