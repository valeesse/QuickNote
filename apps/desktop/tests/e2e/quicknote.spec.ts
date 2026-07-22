import { expect, test } from "@playwright/test";
import { installMockBackend, noteItem } from "./support";

test.beforeEach(async ({ page }) => installMockBackend(page));

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
  const restoredNote = noteItem(page, "中文搜索便签");
  await expect(restoredNote).toBeVisible();
  await restoredNote.click();
  await expect(page.locator(".tiptap img")).toHaveCount(1);

  await page.getByPlaceholder("搜索便签...").fill("中文搜索");
  await expect(noteItem(page, "中文搜索便签")).toBeVisible();
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
  await expect(noteItem(page, "不会丢失的第一条便签")).toBeVisible();
  await expect(noteItem(page, "第二条便签")).toBeVisible();
});

test("switches repeatedly between existing notes", async ({ page }) => {
  await page.evaluate(() => {
    const now = new Date().toISOString();
    localStorage.setItem("quicknote-e2e-db", JSON.stringify(
      ["第一条", "第二条", "第三条"].map((title, index) => ({
        id: `note-${index + 1}`,
        title,
        content: `<p>${title}</p>`,
        is_pinned: false,
        created_at: now,
        updated_at: now,
        version: 1,
        is_deleted: false,
        tags: [],
      })),
    ));
  });
  await page.reload();

  for (const title of ["第一条", "第二条", "第三条", "第一条"]) {
    await noteItem(page, title).click();
    await expect(page.locator(".tiptap")).toContainText(title);
  }
});

test("switching notes does not change their modification time", async ({ page }) => {
  const originalUpdatedAt = "2024-01-02T03:04:05.000Z";
  await page.evaluate((updatedAt) => {
    localStorage.setItem("quicknote-e2e-db", JSON.stringify(
      ["第一条旧便签", "第二条旧便签"].map((title, index) => ({
        id: `unchanged-note-${index + 1}`,
        title,
        content: `<p>${title}</p>`,
        is_pinned: false,
        created_at: updatedAt,
        updated_at: updatedAt,
        version: 1,
        is_deleted: false,
        tags: [],
      })),
    ));
  }, originalUpdatedAt);
  await page.reload();

  await noteItem(page, "第一条旧便签").click();
  await expect(page.locator(".tiptap")).toContainText("第一条旧便签");
  await noteItem(page, "第二条旧便签").click();
  await expect(page.locator(".tiptap")).toContainText("第二条旧便签");
  await page.waitForTimeout(700);

  const notes = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("quicknote-e2e-db") || "[]") as Array<{
      updated_at: string;
      version: number;
    }>,
  );
  expect(notes.every((note) => note.updated_at === originalUpdatedAt)).toBe(true);
  expect(notes.every((note) => note.version === 1)).toBe(true);
});

test("keeps the latest selection while the previous note is still saving", async ({ page }) => {
  await page.evaluate(() => {
    const now = new Date().toISOString();
    localStorage.setItem("quicknote-e2e-save-delay", "400");
    localStorage.setItem("quicknote-e2e-db", JSON.stringify(
      ["待保存便签", "中间便签", "最终便签"].map((title, index) => ({
        id: `saving-note-${index + 1}`,
        title,
        content: `<p>${title}</p>`,
        is_pinned: false,
        created_at: now,
        updated_at: now,
        version: 1,
        is_deleted: false,
        tags: [],
      })),
    ));
  });
  await page.reload();

  await noteItem(page, "待保存便签").click();
  const editor = page.locator(".tiptap").first();
  await editor.click();
  await page.keyboard.press("End");
  await page.keyboard.type("修改");
  await expect(page.getByText("保存中")).toBeVisible();

  await noteItem(page, "中间便签").click();
  await noteItem(page, "最终便签").click();

  await expect(editor).toContainText("最终便签", { timeout: 5_000 });
  await expect(editor).not.toContainText("中间便签");
});

test("switches from search results to a tag filter", async ({ page }) => {
  await page.evaluate(() => {
    const now = new Date().toISOString();
    localStorage.setItem("quicknote-e2e-db", JSON.stringify([
      {
        id: "work-note",
        title: "工作记录",
        content: "<p>工作记录</p>",
        is_pinned: false,
        created_at: now,
        updated_at: now,
        version: 1,
        is_deleted: false,
        tags: ["工作"],
      },
      {
        id: "personal-note",
        title: "个人记录",
        content: "<p>个人记录</p>",
        is_pinned: false,
        created_at: now,
        updated_at: now,
        version: 1,
        is_deleted: false,
        tags: ["个人"],
      },
    ]));
  });
  await page.reload();

  const search = page.getByPlaceholder("搜索便签...");
  await search.fill("个人");
  const personalNote = noteItem(page, "个人记录");
  await expect(personalNote).toBeVisible();
  await personalNote.click();
  await expect(page.locator(".tiptap")).toContainText("个人记录");

  await page.getByTitle("#工作").click();

  await expect(search).toHaveValue("");
  await expect(noteItem(page, "工作记录")).toBeVisible();
  await expect(noteItem(page, "个人记录")).toHaveCount(0);
  await expect(page.locator(".tiptap")).toContainText("工作记录");
});

test("skips automatic sync when there are no local changes", async ({ page }) => {
  await page.evaluate(() => localStorage.setItem("quicknote-e2e-sync-enabled", "true"));
  await page.reload();
  await expect
    .poll(() => page.evaluate(() => Number(localStorage.getItem("quicknote-e2e-sync-count") || 0)))
    .toBe(1);

  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect
    .poll(() => page.evaluate(() => Number(localStorage.getItem("quicknote-e2e-sync-check-count") || 0)))
    .toBeGreaterThan(0);
  expect(await page.evaluate(() => Number(localStorage.getItem("quicknote-e2e-sync-count") || 0))).toBe(1);
});

test("pulls a remote change even when this device has no local changes", async ({ page }) => {
  await page.evaluate(() => localStorage.setItem("quicknote-e2e-sync-enabled", "true"));
  await page.reload();
  await expect
    .poll(() => page.evaluate(() => Number(localStorage.getItem("quicknote-e2e-sync-count") || 0)))
    .toBe(1);

  await page.evaluate(() => {
    localStorage.setItem("quicknote-e2e-sync-remote-dirty", "true");
    window.dispatchEvent(new Event("focus"));
  });
  await expect
    .poll(() => page.evaluate(() => Number(localStorage.getItem("quicknote-e2e-sync-count") || 0)))
    .toBe(2);
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
  const restored = noteItem(page, "第一段第二段");
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
  await expect(noteItem(page, "崩溃后恢复的草稿")).toBeVisible();
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

test("keeps Yjs authoritative when a synced HTML projection is stale", async ({ page }) => {
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
  await noteItem(page, "本地版本").click();
  await expect(editor).toContainText("本地版本");
  await page.waitForTimeout(1_500);
  await expect(editor).toContainText("本地版本");
  await expect(editor).not.toContainText("远端新版本");
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

  await expect(page.getByText("https://example.com/shared").last()).toBeVisible();
  await expect(page.getByText("链接", { exact: true }).last()).toBeVisible();
  await page.getByPlaceholder("搜索剪贴板历史").fill("example.com");
  await expect(page.getByText("https://example.com/shared").last()).toBeVisible();

  await page.getByTitle("固定").click();
  await expect(page.getByTitle("取消固定")).toBeVisible();
  await page.getByTitle("复制到剪贴板").click();
  await expect(page.getByText("已复制")).toBeVisible();

  await page.getByTitle("删除").click();
  await expect(page.getByText("暂无剪贴板记录")).toBeVisible();
});

test("pages clipboard history without loading the full collection", async ({ page }) => {
  await page.evaluate(() => {
    const items = Array.from({ length: 55 }, (_, index) => {
      const timestamp = new Date(Date.UTC(2026, 0, 1, 0, 0, 55 - index)).toISOString();
      return {
        id: `clipboard-page-${index}`,
        kind: "text",
        content: `分页剪贴板 ${index}`,
        preview: `分页剪贴板 ${index}`,
        source_device: "e2e-device",
        created_at: timestamp,
        updated_at: timestamp,
        last_copied_at: timestamp,
        capture_count: 1,
        is_pinned: false,
        is_deleted: false,
      };
    });
    localStorage.setItem("quicknote-e2e-clipboard-db", JSON.stringify(items));
  });
  await page.reload();
  await page.getByRole("button", { name: "剪贴板" }).click();

  await expect(page.locator(".clipboard-card")).toHaveCount(50);
  await expect(page.getByText("分页剪贴板 54", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "加载更多" }).click();
  await expect(page.locator(".clipboard-card")).toHaveCount(55);
  await expect(page.locator(".clipboard-card").getByText("分页剪贴板 54", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "加载更多" })).toHaveCount(0);
});

test("hydrates local clipboard images without loading remote trackers", async ({ page }) => {
  await page.evaluate(() => {
    const timestamp = new Date().toISOString();
    localStorage.setItem("quicknote-e2e-clipboard-db", JSON.stringify([{
      id: "rich-image-item",
      kind: "rich",
      content: '<p>图文混排</p><img src="attachment://local-image" alt="本地图片"><img src="https://tracker.invalid/pixel.png" alt="远程图片">',
      preview: "图文混排",
      source_device: "e2e-device",
      created_at: timestamp,
      updated_at: timestamp,
      last_copied_at: timestamp,
      capture_count: 1,
      is_pinned: false,
      is_deleted: false,
    }]));
  });
  await page.reload();
  await page.getByRole("button", { name: "剪贴板" }).click();

  const localImage = page.locator('.clipboard-card img[alt="本地图片"]');
  const remoteImage = page.locator('.clipboard-card img[alt="远程图片"]');
  await expect(localImage).toHaveAttribute("src", /asset:\/\/localhost/);
  await expect(remoteImage).not.toHaveAttribute("src", /.+/);
});

test("renders formatted text in a compact masonry layout", async ({ page }) => {
  await page.setViewportSize({ width: 1_000, height: 900 });
  await page.evaluate(() => {
    const recent = new Date().toISOString();
    const older = new Date(Date.now() - 1_000).toISOString();
    const oldest = new Date(Date.now() - 2_000).toISOString();
    const item = (id: string, content: string, timestamp: string) => ({
      id,
      kind: "rich",
      content,
      preview: content,
      source_device: "e2e-device",
      created_at: timestamp,
      updated_at: timestamp,
      last_copied_at: timestamp,
      capture_count: 1,
      is_pinned: false,
      is_deleted: false,
    });
    localStorage.setItem("quicknote-e2e-clipboard-db", JSON.stringify([
      item(
        "long-formatted",
        `<pre>${Array.from({ length: 18 }, (_, index) => `格式代码行 ${index + 1}`).join("\n")}</pre>`,
        recent,
      ),
      item(
        "short-formatted",
        '<p style="background-color: rgb(254, 226, 226)"><span style="font-weight: bold; color: rgb(255, 0, 0); position: fixed">短富文本</span></p>',
        older,
      ),
      item("following-formatted", "<p>后续瀑布流卡片</p>", oldest),
    ]));
  });
  await page.reload();
  await page.getByRole("button", { name: "剪贴板" }).click();

  const shortCard = page.locator(".clipboard-card").filter({ hasText: "短富文本" });
  const longCard = page.locator(".clipboard-card").filter({ hasText: "格式代码行 18" });
  const followingCard = page.locator(".clipboard-card").filter({ hasText: "后续瀑布流卡片" });
  await expect(shortCard.getByText("富文本", { exact: true })).toBeVisible();
  await expect(longCard.getByText("富文本", { exact: true })).toBeVisible();
  const formattedSpan = shortCard.locator("span").filter({ hasText: "短富文本" }).last();
  const coloredBlock = shortCard.locator('p[style*="background-color"]');
  await expect(formattedSpan).toHaveCSS("font-weight", "700");
  await expect(formattedSpan).toHaveCSS("color", "rgb(255, 0, 0)");
  await expect(formattedSpan).not.toHaveCSS("position", "fixed");
  await expect(coloredBlock).toHaveCSS("padding-left", "10px");
  const [shortHeight, longHeight] = await Promise.all([
    shortCard.evaluate((element) => element.getBoundingClientRect().height),
    longCard.evaluate((element) => element.getBoundingClientRect().height),
  ]);
  expect(shortHeight).toBeLessThan(longHeight - 40);
  await expect.poll(async () => {
    const [shortBox, longBox, followingBox] = await Promise.all([
      shortCard.boundingBox(),
      longCard.boundingBox(),
      followingCard.boundingBox(),
    ]);
    return Boolean(
      shortBox && longBox && followingBox
      && Math.abs(shortBox.x - followingBox.x) < 2
      && followingBox.y >= shortBox.y + shortBox.height
      && followingBox.y < longBox.y + longBox.height,
    );
  }).toBe(true);
});

test("provides mobile navigation and manual clipboard capture", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole("navigation")).toBeVisible();
  await page.evaluate(() => localStorage.setItem("quicknote-e2e-system-clipboard", "移动端剪贴板"));
  await page.getByRole("navigation").getByRole("button", { name: "剪贴板" }).click();
  await page.getByRole("button", { name: "读取当前剪贴板" }).click();
  await expect(page.getByText("移动端剪贴板").last()).toBeVisible();

  await page.getByRole("navigation").getByRole("button", { name: "便签" }).click();
  await expect(page.getByRole("button", { name: "新建", exact: true })).toBeVisible();
  await expect(page.locator("select")).toBeVisible();
});
