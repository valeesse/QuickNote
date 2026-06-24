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
          const item = items.find((entry) => entry.id === args.id);
          if (item) item.is_deleted = true;
          saveClipboard(items);
          return Boolean(item);
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
        if (cmd === "sync_now") {
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
  await expect(page.getByText("保存中")).toBeVisible();
  await expect(page.getByText("已保存")).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() => localStorage.getItem("quicknote-e2e-db") || "")
    )
    .toContain("attachment://");

  await page.reload();
  const restoredNote = page.getByRole("heading", { name: "中文搜索便签" });
  await expect(restoredNote).toBeVisible();
  await restoredNote.click();
  await expect(page.locator(".tiptap img")).toHaveCount(1);

  await page.getByPlaceholder("搜索便签...").fill("中文搜索");
  await expect(page.getByRole("heading", { name: "中文搜索便签" })).toBeVisible();
});

test("flushes the current draft before switching notes", async ({ page }) => {
  await page.getByTitle("新建便签 (Ctrl+N)").click();
  const editor = page.locator(".tiptap").first();
  await editor.click();
  await page.keyboard.type("不会丢失的第一条便签");

  await page.getByTitle("新建便签 (Ctrl+N)").click();
  await editor.click();
  await page.keyboard.type("第二条便签");
  await expect(page.getByText("已保存")).toBeVisible();

  await page.reload();
  await expect(page.getByRole("heading", { name: "不会丢失的第一条便签" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "第二条便签" })).toBeVisible();
});

test("does not roll back newer typing when an older save finishes", async ({ page }) => {
  await page.evaluate(() => localStorage.setItem("quicknote-e2e-save-delay", "400"));
  await page.getByTitle("新建便签 (Ctrl+N)").click();
  const editor = page.locator(".tiptap").first();
  await editor.click();
  await page.keyboard.type("第一段");
  await expect(page.getByText("保存中")).toBeVisible();
  await page.waitForTimeout(550);
  await page.keyboard.type("第二段");
  await expect(editor).toContainText("第一段第二段");
  await expect(page.getByText("已保存")).toBeVisible({ timeout: 5_000 });

  await page.reload();
  const restored = page.getByRole("heading", { name: "第一段第二段" });
  await expect(restored).toBeVisible();
});

test("recovers a journaled draft when reloaded before debounce", async ({ page }) => {
  await page.getByTitle("新建便签 (Ctrl+N)").click();
  const editor = page.locator(".tiptap").first();
  await editor.click();
  await page.keyboard.type("崩溃后恢复的草稿");
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("quicknote-draft-journal-v1")))
    .toContain("崩溃后恢复的草稿");

  await page.reload();
  await expect(page.getByRole("heading", { name: "崩溃后恢复的草稿" })).toBeVisible();
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("quicknote-draft-journal-v1")))
    .toBeNull();
});

test("keeps a failed draft journal until a later retry succeeds", async ({ page }) => {
  await page.evaluate(() => localStorage.setItem("quicknote-e2e-save-failures", "3"));
  await page.getByTitle("新建便签 (Ctrl+N)").click();
  const editor = page.locator(".tiptap").first();
  await editor.click();
  await page.keyboard.type("失败后仍保留");

  await expect(page.getByText("simulated save failure", { exact: true })).toBeVisible({ timeout: 5_000 });
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("quicknote-draft-journal-v1")))
    .toContain("失败后仍保留");

  await page.evaluate(() => window.dispatchEvent(new Event("online")));
  await expect(page.getByText("已保存")).toBeVisible();
  await expect
    .poll(() => page.evaluate(() => localStorage.getItem("quicknote-draft-journal-v1")))
    .toBeNull();
});

test("refreshes the active editor after pulling a remote version", async ({ page }) => {
  await page.getByTitle("新建便签 (Ctrl+N)").click();
  const editor = page.locator(".tiptap").first();
  await editor.click();
  await page.keyboard.type("本地版本");
  await expect(page.getByText("已保存")).toBeVisible();

  await page.evaluate(() => {
    localStorage.setItem("quicknote-e2e-sync-enabled", "true");
    localStorage.setItem("quicknote-e2e-sync-delay", "800");
    localStorage.setItem("quicknote-e2e-remote-content", "<p>远端新版本</p>");
  });
  await page.reload();
  await page.getByRole("heading", { name: "本地版本" }).click();
  await expect(editor).toContainText("本地版本");
  await expect(editor).toContainText("远端新版本", { timeout: 5_000 });
  await expect(editor).not.toContainText("本地版本");
});

test("imports markdown and renders markdown shortcuts live", async ({ page }) => {
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

  const editor = page.locator(".tiptap").first();
  await editor.click();
  await page.keyboard.press(process.platform === "darwin" ? "Meta+A" : "Control+A");
  await page.keyboard.type("# 实时标题");
  await expect(page.locator(".tiptap h1")).toHaveText("实时标题");

  await page.keyboard.press("Enter");
  await page.keyboard.type("无*b* 无~~实时~~ 无**粗体** 无==高亮== 无`代码` ");
  await expect(page.locator(".tiptap em")).toHaveText("b");
  await expect(page.locator(".tiptap s")).toHaveText("实时");
  await expect(page.locator(".tiptap strong")).toHaveText("粗体");
  await expect(page.locator(".tiptap mark")).toHaveText("高亮");
  await expect(page.locator(".tiptap code")).toHaveText("代码");
  await expect(editor).toContainText("无b");

  await expect(page.getByTitle("复制为 Markdown")).toBeVisible();
  await expect(page.getByTitle("导出 Markdown")).toBeVisible();
});

test("captures, searches, pins, copies, and deletes clipboard history", async ({ page }) => {
  await page.evaluate(() => localStorage.setItem("quicknote-e2e-system-clipboard", "https://example.com/shared"));
  await page.getByRole("button", { name: "剪贴板" }).click();
  await page.getByRole("button", { name: "读取当前剪贴板" }).click();

  await expect(page.getByText("https://example.com/shared")).toBeVisible();
  await expect(page.getByText("链接", { exact: true })).toBeVisible();
  await page.getByPlaceholder("搜索剪贴板历史").fill("example.com");
  await expect(page.getByText("https://example.com/shared")).toBeVisible();

  await page.getByTitle("固定").click();
  await expect(page.getByTitle("取消固定")).toBeVisible();
  await page.getByTitle("复制到剪贴板").click();
  await expect(page.getByText("已复制")).toBeVisible();

  await page.getByTitle("删除").click();
  await expect(page.getByText("暂无剪贴板记录")).toBeVisible();
});

test("provides mobile navigation and manual clipboard capture", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole("navigation")).toBeVisible();
  await page.evaluate(() => localStorage.setItem("quicknote-e2e-system-clipboard", "移动端剪贴板"));
  await page.getByRole("navigation").getByRole("button", { name: "剪贴板" }).click();
  await page.getByRole("button", { name: "读取当前剪贴板" }).click();
  await expect(page.getByText("移动端剪贴板")).toBeVisible();

  await page.getByRole("navigation").getByRole("button", { name: "便签" }).click();
  await expect(page.getByRole("button", { name: "新建", exact: true })).toBeVisible();
  await expect(page.locator("select")).toBeVisible();
});
