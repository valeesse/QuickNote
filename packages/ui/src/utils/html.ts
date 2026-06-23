/**
 * Strip HTML tags from content, returning plain text.
 */
export function stripHtml(html: string): string {
  return html.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim() || "空便签";
}

/**
 * Strip markdown/HTML formatting for preview display.
 */
export function stripMarkdown(text: string): string {
  const withoutImages = text.replace(/<img\b[^>]*>/gi, " [图片] ");
  const plainText = withoutImages
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, " ")
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");

  return plainText
    .replace(/#{1,6}\s/g, "")
    .replace(/[*_~`]/g, "")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/!\[([^\]]*)\]\([^)]+\)/g, "[图片]")
    .replace(/\s+/g, " ")
    .trim();
}
