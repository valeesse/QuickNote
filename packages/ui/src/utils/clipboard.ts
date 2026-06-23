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

export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
