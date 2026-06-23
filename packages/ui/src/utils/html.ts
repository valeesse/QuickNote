/**
 * Strip HTML tags from content, returning plain text.
 * Also handles truncated tags from server-side LEFT() truncation.
 */
export function stripHtml(html: string): string {
  return html
    .replace(/<[^>]+>/g, " ")
    .replace(/<[^>]*$/, " ")
    .replace(/\s+/g, " ")
    .trim() || "空便签";
}

/** Decode common HTML entities to their character equivalents. */
function decodeEntities(text: string): string {
  return text
    .replace(/&nbsp;/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&#(\d+);/g, (_m, code) => String.fromCharCode(Number(code)))
    .replace(/&#x([0-9a-fA-F]+);/g, (_m, code) => String.fromCharCode(parseInt(code, 16)));
}

/**
 * Strip markdown/HTML formatting for preview display.
 * Uses DOMParser when available (browser) for reliable HTML text extraction,
 * falls back to regex for non-browser environments.
 */
export function stripMarkdown(text: string): string {
  // Browser path: DOMParser handles all HTML correctly, including truncated tags
  if (typeof DOMParser !== "undefined") {
    // Close any truncated tag at the end to avoid DOMParser dropping trailing text
    const safe = text.replace(/<[^>]*$/, "");
    const doc = new DOMParser().parseFromString(safe, "text/html");

    // Remove script/style elements entirely
    doc.querySelectorAll("script, style").forEach((el) => el.remove());

    // Replace <img> with placeholder before extracting text
    doc.querySelectorAll("img").forEach((el) => {
      el.replaceWith(doc.createTextNode(" [图片] "));
    });
    // Replace <br> with newline so textContent preserves line breaks
    doc.querySelectorAll("br").forEach((el) => {
      el.replaceWith(doc.createTextNode("\n"));
    });

    return decodeEntities(doc.body.textContent || "")
      .replace(/#{1,6}\s/g, "")
      .replace(/[*_~`]/g, "")
      .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
      .replace(/!\[([^\]]*)\]\([^)]+\)/g, "[图片]")
      .replace(/\s+/g, " ")
      .trim();
  }

  // Fallback: pure regex for non-browser environments
  const withoutImages = text.replace(/<img\b[^>]*>/gi, " [图片] ");
  const plainText = withoutImages
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, " ")
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/<[^>]*$/, " ")
    .replace(/&nbsp;/g, " ")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&#(\d+);/g, (_m, code) => String.fromCharCode(Number(code)))
    .replace(/&#x([0-9a-fA-F]+);/g, (_m, code) => String.fromCharCode(parseInt(code, 16)));

  return plainText
    .replace(/#{1,6}\s/g, "")
    .replace(/[*_~`]/g, "")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/!\[([^\]]*)\]\([^)]+\)/g, "[图片]")
    .replace(/\s+/g, " ")
    .trim();
}
