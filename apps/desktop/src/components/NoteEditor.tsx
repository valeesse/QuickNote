import { useCallback } from "react";
import { SharedNoteEditor } from "@ui/index";
import type { Attachment, Note, SaveStatus, TagSummary } from "@/types";

interface NoteEditorProps {
  note: Note;
  onUpdate: (id: string, content: string, yjsState?: number[]) => void;
  onSaveAttachment: (dataUrl: string, filename: string) => Promise<Attachment>;
  onResolveAttachment: (id: string) => Promise<string>;
  onOpenHistory: () => void;
  onUpdateTags: (noteId: string, tags: string[]) => void;
  tags: TagSummary[];
  saveStatus: SaveStatus;
  errorMessage: string | null;
  isSyncing: boolean;
}

export function NoteEditor(props: NoteEditorProps) {
  const { onSaveAttachment, onResolveAttachment } = props;
  return (
    <SharedNoteEditor
      {...props}
      saveImage={useCallback(async (file: File, dataUrl: string) => {
        const attachment = await onSaveAttachment(dataUrl, file.name);
        return { src: `attachment://${attachment.id}`, alt: file.name, attachmentId: attachment.id };
      }, [onSaveAttachment])}
      resolveImageSrc={onResolveAttachment}
      serializeContent={canonicalizeAttachmentReferences}
      hydrateContent={useCallback(
        (content: string) => hydrateAttachmentReferences(content, onResolveAttachment),
        [onResolveAttachment],
      )}
      shouldMigrateContent={useCallback((note: Note) => note.content.includes("data:image/"), [])}
      migrateContent={useCallback(
        (content: string) => migrateDataUrlImages(content, onSaveAttachment),
        [onSaveAttachment],
      )}
    />
  );
}

async function migrateDataUrlImages(
  content: string,
  saveAttachment: (dataUrl: string, filename: string) => Promise<Attachment>,
): Promise<string> {
  const doc = new DOMParser().parseFromString(`<main>${content}</main>`, "text/html");
  const images = Array.from(doc.querySelectorAll<HTMLImageElement>("img[src^='data:image/']"));
  for (const [index, image] of images.entries()) {
    const src = image.getAttribute("src");
    if (!src) continue;
    const attachment = await saveAttachment(src, image.alt || `image-${index + 1}.webp`);
    image.src = `attachment://${attachment.id}`;
    image.dataset.attachmentId = attachment.id;
  }
  return doc.querySelector("main")?.innerHTML ?? content;
}

function canonicalizeAttachmentReferences(content: string): string {
  if (!content.includes("data-attachment-id")) return content;
  const doc = new DOMParser().parseFromString(`<main>${content}</main>`, "text/html");
  for (const image of doc.querySelectorAll<HTMLImageElement>("img[data-attachment-id]")) {
    const id = image.dataset.attachmentId;
    if (id) image.src = `attachment://${id}`;
  }
  return doc.querySelector("main")?.innerHTML ?? content;
}

async function hydrateAttachmentReferences(
  content: string,
  resolveAttachment: (id: string) => Promise<string>,
): Promise<string> {
  if (!content.includes("attachment://")) return content;
  const doc = new DOMParser().parseFromString(`<main>${content}</main>`, "text/html");
  const images = Array.from(doc.querySelectorAll<HTMLImageElement>("img[src^='attachment://']"));
  await Promise.all(images.map(async (image) => {
    const id = image.getAttribute("src")?.slice("attachment://".length);
    if (!id) return;
    try {
      image.src = await resolveAttachment(id);
      image.dataset.attachmentId = id;
    } catch {
      image.alt = image.alt || "附件缺失";
    }
  }));
  return doc.querySelector("main")?.innerHTML ?? content;
}
