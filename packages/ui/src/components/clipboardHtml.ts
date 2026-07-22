import { useEffect, useState } from "react";

export function useHydratedClipboardHtml(
  html: string,
  resolveAttachmentSrc?: (id: string) => Promise<string>,
): string {
  const [renderedHtml, setRenderedHtml] = useState(() => sanitizeClipboardHtml(html));

  useEffect(() => {
    let disposed = false;
    let objectUrls: string[] = [];

    async function hydrate() {
      const nextHtml = resolveAttachmentSrc
        ? await hydrateAttachmentReferences(html, resolveAttachmentSrc, (url) => {
            if (disposed) URL.revokeObjectURL(url);
            else objectUrls.push(url);
          })
        : html;
      if (!disposed) setRenderedHtml(sanitizeClipboardHtml(nextHtml));
    }

    void hydrate();
    return () => {
      disposed = true;
      for (const url of objectUrls) URL.revokeObjectURL(url);
      objectUrls = [];
    };
  }, [html, resolveAttachmentSrc]);

  return renderedHtml;
}

async function hydrateAttachmentReferences(
  html: string,
  resolveAttachmentSrc: (id: string) => Promise<string>,
  trackObjectUrl: (url: string) => void,
): Promise<string> {
  if (!html.includes("attachment://") || typeof document === "undefined") return html;
  const doc = new DOMParser().parseFromString(html, "text/html");
  const images = Array.from(doc.querySelectorAll<HTMLImageElement>("img[src^='attachment://']"));
  await Promise.all(images.map(async (image) => {
    const id = image.getAttribute("src")?.slice("attachment://".length);
    if (!id) return;
    try {
      const src = await resolveAttachmentSrc(id);
      if (!src) return;
      image.src = src;
      image.dataset.attachmentId = id;
      if (src.startsWith("blob:")) trackObjectUrl(src);
    } catch {
      image.removeAttribute("src");
      image.alt = image.alt || "附件缺失";
    }
  }));
  return doc.body.innerHTML;
}

export function shortDevice(device: string): string {
  return device ? `设备 ${device.slice(0, 6)}` : "本机";
}

function sanitizeClipboardHtml(html: string): string {
  if (typeof document === "undefined") return "";
  const doc = new DOMParser().parseFromString(html, "text/html");
  const allowedTags = new Set([
    "A",
    "B",
    "BLOCKQUOTE",
    "BR",
    "CODE",
    "DIV",
    "EM",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "HR",
    "I",
    "IMG",
    "LI",
    "MARK",
    "OL",
    "P",
    "PRE",
    "S",
    "SPAN",
    "STRONG",
    "SUB",
    "SUP",
    "TABLE",
    "TBODY",
    "TD",
    "TFOOT",
    "TH",
    "THEAD",
    "TR",
    "U",
    "UL",
  ]);
  const allowedAttrs = new Set(["alt", "colspan", "href", "rowspan", "src", "style", "title"]);

  for (const element of Array.from(doc.body.querySelectorAll("*"))) {
    if (!allowedTags.has(element.tagName)) {
      element.replaceWith(...Array.from(element.childNodes));
      continue;
    }
    for (const attr of Array.from(element.attributes)) {
      const name = attr.name.toLowerCase();
      const value = attr.value.trim();
      if (!allowedAttrs.has(name)) {
        element.removeAttribute(attr.name);
        continue;
      }
      if ((name === "src" || name === "href") && !isSafeClipboardUrl(value, name)) {
        element.removeAttribute(attr.name);
      } else if (name === "style") {
        const style = sanitizeInlineStyle(value);
        if (style) element.setAttribute("style", style);
        else element.removeAttribute("style");
      } else if ((name === "colspan" || name === "rowspan") && !isSafeTableSpan(value)) {
        element.removeAttribute(attr.name);
      }
    }
    if (element.tagName === "A") {
      element.setAttribute("target", "_blank");
      element.setAttribute("rel", "noreferrer");
    }
    if (element.tagName === "IMG") {
      element.setAttribute("loading", "lazy");
      element.setAttribute("decoding", "async");
    }
  }

  return doc.body.innerHTML;
}

const SAFE_STYLE_PROPERTIES = [
  "background-color",
  "color",
  "font-family",
  "font-style",
  "font-weight",
  "text-align",
  "text-decoration",
] as const;

function sanitizeInlineStyle(value: string): string {
  const probe = document.createElement("span");
  probe.setAttribute("style", value);
  const safe: string[] = [];
  for (const property of SAFE_STYLE_PROPERTIES) {
    const candidate = probe.style.getPropertyValue(property).trim();
    if (candidate && isSafeStyleValue(property, candidate)) {
      safe.push(`${property}: ${candidate}`);
    }
  }
  return safe.join("; ");
}

function isSafeStyleValue(property: typeof SAFE_STYLE_PROPERTIES[number], value: string): boolean {
  if (/[;{}]|url\s*\(|expression\s*\(|var\s*\(/i.test(value)) return false;
  if (property === "font-weight") return /^(normal|bold|bolder|lighter|[1-9]00)$/i.test(value);
  if (property === "font-style") return /^(normal|italic|oblique)$/i.test(value);
  if (property === "text-align") return /^(left|right|center|justify|start|end)$/i.test(value);
  if (property === "text-decoration") {
    return /^(none|underline|overline|line-through)(\s+(underline|overline|line-through))*$/i.test(value);
  }
  if (property === "font-family") return /^[\p{L}\p{N}\s,'"_-]+$/u.test(value);
  return /^(#[0-9a-f]{3,8}|[a-z]+|(rgb|rgba|hsl|hsla)\([\d\s.,%+-]+\))$/i.test(value);
}

function isSafeTableSpan(value: string): boolean {
  const span = Number(value);
  return Number.isInteger(span) && span >= 1 && span <= 100;
}

function isSafeClipboardUrl(value: string, attribute: string): boolean {
  if (attribute === "href") {
    return value.startsWith("https://") || value.startsWith("http://");
  }
  return (
    value.startsWith("data:image/") ||
    value.startsWith("asset:") ||
    value.startsWith("http://asset.localhost/") ||
    value.startsWith("blob:")
  );
}
