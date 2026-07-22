import type { ClipboardItem } from "@contracts";

export function clipboardItemToNoteContent(item: ClipboardItem): string {
  if (item.kind === "rich" || item.kind === "image") return item.content;
  if (item.kind === "code") {
    return `<pre><code>${escapeHtml(item.content)}</code></pre>`;
  }
  const text = escapeHtml(item.content)
    .split(/\r?\n/)
    .map((line) => line || "<br>")
    .join("</p><p>");
  return `<p>${text}</p>`;
}

export function clipboardKindLabel(item: ClipboardItem): string {
  if (item.kind === "link") return "链接";
  if (item.kind === "code") return "代码";
  if (item.kind === "image") return "图片";
  if (item.kind === "rich") return hasHtmlImage(item.content) ? "图文" : "富文本";
  return "文本";
}

export function hasHtmlImage(content: string): boolean {
  return /<img(?:\s|>)/i.test(content);
}

export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
