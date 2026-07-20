import { useCallback, useEffect, useRef } from "react";
import { SharedNoteEditor } from "@ui/index";
import type { Note, SaveStatus, TagSummary } from "@/types";
import { attachmentsApi, getBaseUrl } from "@/api/client";

interface NoteEditorProps {
  note: Note;
  onUpdate: (id: string, content: string) => void;
  saveStatus: SaveStatus;
  errorMessage: string | null;
  onOpenHistory?: () => void;
  onUpdateTags: (noteId: string, tags: string[]) => void;
  tags: TagSummary[];
  isSyncing?: boolean;
}

export function NoteEditor(props: NoteEditorProps) {
  const objectUrlsRef = useRef<string[]>([]);
  useEffect(() => () => {
    for (const url of objectUrlsRef.current) URL.revokeObjectURL(url);
  }, []);
  return (
    <SharedNoteEditor
      {...props}
      websocketUrl={collaborationUrl(props.note.id)}
      saveImage={useCallback(async (file: File, dataUrl: string) => {
        const { bytes, mimeType } = decodeDataUrl(dataUrl);
        const id = await sha256(bytes);
        await attachmentsApi.upload(id, bytes, mimeType);
        return { src: `attachment://${id}`, alt: file.name, attachmentId: id };
      }, [])}
      resolveImageSrc={useCallback(
        (id: string) => resolveAttachment(id, objectUrlsRef.current),
        [],
      )}
      serializeContent={serializeAttachments}
      hydrateContent={useCallback(
        (content: string) => hydrateAttachments(content, objectUrlsRef.current),
        [],
      )}
    />
  );
}

async function resolveAttachment(id: string, objectUrls: string[]): Promise<string> {
  const blob = await attachmentsApi.download(id);
  const url = URL.createObjectURL(blob);
  objectUrls.push(url);
  return url;
}

function collaborationUrl(noteId: string): string {
  const base = getBaseUrl();
  const origin = base ? new URL(base, window.location.origin) : window.location;
  const protocol = origin.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${origin.host}/api/collab/notes/${encodeURIComponent(noteId)}/ws`;
}

function decodeDataUrl(dataUrl: string): { bytes: Uint8Array; mimeType: string } {
  const [header, payload] = dataUrl.split(",", 2);
  if (!header || !payload) throw new Error("Invalid image data");
  const bytes = Uint8Array.from(atob(payload), (character) => character.charCodeAt(0));
  return { bytes, mimeType: header.slice(5).split(";", 1)[0] || "application/octet-stream" };
}

async function sha256(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes as BufferSource);
  return Array.from(new Uint8Array(digest), (value) => value.toString(16).padStart(2, "0")).join("");
}

function serializeAttachments(content: string): string {
  if (!content.includes("data-attachment-id")) return content;
  const doc = new DOMParser().parseFromString(content, "text/html");
  for (const image of doc.querySelectorAll<HTMLImageElement>("img[data-attachment-id]")) {
    const id = image.dataset.attachmentId;
    if (id) image.src = `attachment://${id}`;
  }
  return doc.body.innerHTML;
}

async function hydrateAttachments(content: string, objectUrls: string[]): Promise<string> {
  for (const url of objectUrls.splice(0)) URL.revokeObjectURL(url);
  if (!content.includes("attachment://")) return content;
  const doc = new DOMParser().parseFromString(content, "text/html");
  const images = Array.from(doc.querySelectorAll<HTMLImageElement>("img[src^='attachment://']"));
  await Promise.all(images.map(async (image) => {
    const id = image.getAttribute("src")?.slice("attachment://".length);
    if (!id) return;
    const blob = await attachmentsApi.download(id);
    const url = URL.createObjectURL(blob);
    objectUrls.push(url);
    image.src = url;
    image.dataset.attachmentId = id;
  }));
  return doc.body.innerHTML;
}
