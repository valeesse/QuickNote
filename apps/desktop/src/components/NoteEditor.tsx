import { useCallback, useEffect } from "react";
import Image from "@tiptap/extension-image";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Highlight from "@tiptap/extension-highlight";
import Typography from "@tiptap/extension-typography";
import { Markdown } from "@tiptap/markdown";
import StarterKit from "@tiptap/starter-kit";
import { useEditor, EditorContent } from "@tiptap/react";
import { EditorShell, InlineMarkdownMarkRules, createAttachmentImageExtension, useAttachmentEditorBridge } from "@ui/index";
import type { Attachment, Note, SaveStatus } from "@/types";

const AttachmentImage = createAttachmentImageExtension(Image);

interface NoteEditorProps {
  note: Note;
  onUpdate: (id: string, content: string) => void;
  onSaveAttachment: (dataUrl: string, filename: string) => Promise<Attachment>;
  onResolveAttachment: (id: string) => Promise<string>;
  onOpenHistory: () => void;
  saveStatus: SaveStatus;
  errorMessage: string | null;
  isSyncing: boolean;
}

export function NoteEditor({
  note,
  onUpdate,
  onSaveAttachment,
  onResolveAttachment,
  onOpenHistory,
  saveStatus,
  errorMessage,
  isSyncing,
}: NoteEditorProps) {
  const bridge = useAttachmentEditorBridge({
    note,
    isSyncing,
    onUpdate,
    serializeContent: canonicalizeAttachmentReferences,
    hydrateContent: useCallback(
      (content: string) => hydrateAttachmentReferences(content, onResolveAttachment),
      [onResolveAttachment],
    ),
    saveImage: useCallback(async (file: File, dataUrl: string) => {
      const attachment = await onSaveAttachment(dataUrl, file.name);
      return {
        src: attachment.path,
        alt: file.name,
        attachmentId: attachment.id,
      };
    }, [onSaveAttachment]),
    shouldMigrateContent: useCallback((nextNote: Note) => nextNote.content.includes("data:image/"), []),
    migrateContent: useCallback(
      (content: string) => migrateDataUrlImages(content, onSaveAttachment),
      [onSaveAttachment],
    ),
  });

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: { levels: [1, 2, 3] },
        codeBlock: { HTMLAttributes: { class: "language-plaintext" } },
      }),
      AttachmentImage.configure({
        inline: false,
        allowBase64: true,
        HTMLAttributes: { class: "rounded-lg max-w-full" },
      }),
      Placeholder.configure({ placeholder: "开始记录你的想法..." }),
      TaskList,
      TaskItem.configure({ nested: true }),
      Highlight.configure({ multicolor: false }),
      Typography,
      InlineMarkdownMarkRules,
      Markdown.configure({ indentation: { style: "space", size: 2 } }),
    ],
    content: note.content.includes("attachment://") ? "" : note.content || "",
    onUpdate: ({ editor }) => bridge.handleEditorUpdate(editor),
    editorProps: bridge.editorProps,
  });

  useEffect(() => {
    bridge.setEditor(editor);
  }, [bridge.setEditor, editor]);

  if (!editor) return null;

  return (
    <EditorShell
      editor={editor}
      note={note}
      saveStatus={saveStatus}
      errorMessage={errorMessage}
      isSyncing={isSyncing}
      onInsertImage={bridge.addImageFromFile}
      findReplace={bridge.findReplace}
      onOpenHistory={onOpenHistory}
    >
      <EditorContent editor={editor} />
    </EditorShell>
  );
}

// ── Attachment helpers (desktop-specific, using Tauri IPC) ──

async function migrateDataUrlImages(
  content: string,
  saveAttachment: (dataUrl: string, filename: string) => Promise<Attachment>
): Promise<string> {
  const doc = new DOMParser().parseFromString(`<main>${content}</main>`, "text/html");
  const images = Array.from(doc.querySelectorAll<HTMLImageElement>("img[src^='data:image/']"));

  for (const [index, image] of images.entries()) {
    const src = image.getAttribute("src");
    if (!src) continue;
    const attachment = await saveAttachment(src, image.getAttribute("alt") || `image-${index + 1}.webp`);
    image.setAttribute("src", `attachment://${attachment.id}`);
    image.setAttribute("data-attachment-id", attachment.id);
  }

  return doc.querySelector("main")?.innerHTML ?? content;
}

function canonicalizeAttachmentReferences(content: string): string {
  if (!content.includes("data-attachment-id")) return content;
  const doc = new DOMParser().parseFromString(`<main>${content}</main>`, "text/html");
  for (const image of doc.querySelectorAll<HTMLImageElement>("img[data-attachment-id]")) {
    const id = image.dataset.attachmentId;
    if (id) image.setAttribute("src", `attachment://${id}`);
  }
  return doc.querySelector("main")?.innerHTML ?? content;
}

async function hydrateAttachmentReferences(
  content: string,
  resolveAttachment: (id: string) => Promise<string>
): Promise<string> {
  if (!content.includes("attachment://")) return content;
  const doc = new DOMParser().parseFromString(`<main>${content}</main>`, "text/html");
  const images = Array.from(doc.querySelectorAll<HTMLImageElement>("img[src^='attachment://']"));
  await Promise.all(
    images.map(async (image) => {
      const id = image.getAttribute("src")?.slice("attachment://".length);
      if (!id) return;
      try {
        image.src = await resolveAttachment(id);
        image.dataset.attachmentId = id;
      } catch {
        image.alt = image.alt || "附件缺失";
      }
    })
  );
  return doc.querySelector("main")?.innerHTML ?? content;
}
