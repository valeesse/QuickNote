import { expect, test, type BrowserContext, type Page } from "@playwright/test";

const password = "collaboration-test-password";

async function authenticate(page: Page, email: string, register = false) {
  await page.goto("/");
  if (register) await page.getByRole("button", { name: "立即注册" }).click();
  await page.getByPlaceholder("user@example.com").fill(email);
  await page.getByPlaceholder("至少 10 个字符").fill(password);
  await page.getByRole("button", { name: register ? "注册" : "登录", exact: true }).click();
  await expect(page.getByRole("button", { name: "新建便签" }).first()).toBeVisible();
}

async function createLegacyNote(page: Page, content: string): Promise<string> {
  return page.evaluate(async (html) => {
    const response = await fetch("/api/notes", {
      method: "POST",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ content: html }),
    });
    if (!response.ok) throw new Error(await response.text());
    return (await response.json()).id as string;
  }, content);
}

async function openNote(page: Page, id: string) {
  await page.goto("/");
  const summary = page.locator(`[data-note-id="${id}"]`);
  const editor = page.locator(".tiptap");
  await expect(summary.or(editor)).toBeVisible();
  if (!(await editor.isVisible())) await summary.click();
  await expect(editor).toBeVisible();
}

async function loginSecond(context: BrowserContext, email: string): Promise<Page> {
  const page = await context.newPage();
  await authenticate(page, email);
  return page;
}

test("Yjs source bootstraps once, merges clients, and restores an offline browser draft", async ({ browser }) => {
  const email = `collab-${Date.now()}@example.com`;
  const firstContext = await browser.newContext();
  const secondContext = await browser.newContext();
  const first = await firstContext.newPage();
  await authenticate(first, email, true);
  const noteId = await createLegacyNote(first, "<p>legacy-source-once</p>");
  const second = await loginSecond(secondContext, email);

  await Promise.all([openNote(first, noteId), openNote(second, noteId)]);
  await expect(first.locator(".tiptap")).toContainText("legacy-source-once");
  await expect(second.locator(".tiptap")).toContainText("legacy-source-once");
  await expect(first.locator(".tiptap")).not.toContainText("legacy-source-oncelegacy-source-once");

  await Promise.all([
    first.locator(".tiptap").press("End").then(() => first.locator(".tiptap").type(" alpha-live")),
    second.locator(".tiptap").press("End").then(() => second.locator(".tiptap").type(" beta-live")),
  ]);
  await expect(first.locator(".tiptap")).toContainText("alpha-live");
  await expect(first.locator(".tiptap")).toContainText("beta-live");
  await expect(second.locator(".tiptap")).toContainText("alpha-live");
  await expect(second.locator(".tiptap")).toContainText("beta-live");
  await expect(first.getByText("已保存", { exact: true })).toBeVisible();

  await secondContext.setOffline(true);
  await second.locator(".tiptap").press("End");
  await second.locator(".tiptap").type(" durable-offline");
  await expect(second.locator(".tiptap")).toContainText("durable-offline");
  await second.waitForTimeout(500);
  await second.close();
  await secondContext.setOffline(false);
  const restored = await secondContext.newPage();
  await openNote(restored, noteId);
  await expect(restored.locator(".tiptap")).toContainText("durable-offline");
  await expect(first.locator(".tiptap")).toContainText("durable-offline");
  await expect(restored.getByText("已保存", { exact: true })).toBeVisible();

  const searchMatches = await restored.evaluate(async () => {
    const response = await fetch("/api/notes/search?q=durable-offline", { credentials: "include" });
    return (await response.json()) as Array<{ id: string }>;
  });
  expect(searchMatches.some((note) => note.id === noteId)).toBeTruthy();
  await firstContext.close();
  await secondContext.close();
});
