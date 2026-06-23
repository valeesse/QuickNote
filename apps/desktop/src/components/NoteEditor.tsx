import React, { useCallback, useEffect, useRef } from "react";
import Image from "@tiptap/extension-image";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Highlight from "@tiptap/extension-highlight";
import Typography from "@tiptap/extension-typography";
import { Markdown } from "@tiptap/markdown";
import StarterKit from "@tiptap/starter-kit";
import { useEditor, EditorContent } from "@tiptap/react";
import { Toolbar, InlineMarkdownMarkRules, compressImageToDataUrl, formatSaveStatus } from "@ui/index";
import type { Attachment, Note, SaveStatus } from "@/types";

const AttachmentImage = Image.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      attachmentId: {
        default: null,
        parseHTML: (element) =>
          element.getAttribute("data-attachment-id") ||
          element.getAttribute("src")?.match(/^attachment:\/\/(.+)$/)?.[1] ||
          null,
        renderHTML: (attributes) =>
          attributes.attachmentId
            ? { "data-attachment-id": attributes.attachmentId }
            : {},
      },
    };
  },
});

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
  const noteIdRef = useRef(note.id);
  const onUpdateRef = useRef(onUpdate);
  const editorRef = useRef<any>(null);
  const lastAppliedContentRef = useRef(note.content || "");
  const isApplyingExternalContentRef = useRef(note.content.includes("attachment://"));
  const migratedNotesRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    noteIdRef.current = note.id;
    onUpdateRef.current = onUpdate;
  }, [note.id, onUpdate]);

  const handleImageInsert = useCallback(async (file: File) => {
    try {
      const dataUrl = await compressImageToDataUrl(file);
      const attachment = await onSaveAttachment(dataUrl, file.name);
      editorRef.current?.chain().focus().setImage({
        src: attachment.path,
        alt: file.name,
        attachmentId: attachment.id,
      } as any).run();
    } catch (err) {
      console.error("Image insert failed:", err);
    }
  }, [onSaveAttachment]);

  const addImageFromFile = useCallback(() => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/*";
    input.onchange = async (e: any) => {
      const file = e.target.files?.[0];
      if (file) {
        const dataUrl = await compressImageToDataUrl(file);
        const attachment = await onSaveAttachment(dataUrl, file.name);
        editorRef.current?.chain().focus().setImage({
          src: attachment.path,
          alt: file.name,
          attachmentId: attachment.id,
        } as any).run();
      }
    };
    input.click();
  }, [onSaveAttachment]);

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
    onUpdate: ({ editor }) => {
      if (isApplyingExternalContentRef.current) return;
      const html = canonicalizeAttachmentReferences(editor.getHTML());
      lastAppliedContentRef.current = html;
      onUpdateRef.current(noteIdRef.current, html);
    },
    editorProps: {
      attributes: {
        class: "tiptap prose prose-sm max-w-none focus:outline-none px-8 py-6 min-h-full",
      },
      handlePaste: (_view, event) => {
        const items = event.clipboardData?.items;
        if (items) {
          for (const item of items) {
            if (item.type.startsWith("image/")) {
              event.preventDefault();
              const file = item.getAsFile();
              if (file) handleImageInsert(file);
              return true;
            }
          }
        }
        return false;
      },
      handleDrop: (_view, event) => {
        const files = event.dataTransfer?.files;
        if (files && files.length > 0) {
          for (const file of files) {
            if (file.type.startsWith("image/")) {
              event.preventDefault();
              handleImageInsert(file);
              return true;
            }
          }
        }
        return false;
      },
    },
  });

  useEffect(() => {
    editorRef.current = editor;
  }, [editor]);

  useEffect(() => {
    editor?.setEditable(!isSyncing);
  }, [editor, isSyncing]);

  useEffect(() => {
    if (!editor) return;
    const nextContent = note.content || "";
    let cancelled = false;
    isApplyingExternalContentRef.current = true;
    void (async () => {
      try {
        const hydrated = await hydrateAttachmentReferences(nextContent, onResolveAttachment);
        if (cancelled) return;
        if (nextContent !== lastAppliedContentRef.current || editor.isEmpty) {
          editor.commands.setContent(hydrated, { emitUpdate: false });
          lastAppliedContentRef.current = nextContent;
        }
      } finally {
        if (!cancelled) isApplyingExternalContentRef.current = false;
      }
    })().catch((err) => console.error("Attachment hydration failed:", err));
    return () => { cancelled = true; };
  }, [editor, note.id, note.content, onResolveAttachment]);

  useEffect(() => {
    if (!note.content.includes("data:image/") || migratedNotesRef.current.has(note.id)) return;
    migratedNotesRef.current.add(note.id);

    let cancelled = false;
    const migrate = async () => {
      const nextContent = await migrateDataUrlImages(note.content, onSaveAttachment);
      if (!cancelled && nextContent !== note.content) {
        const hydrated = await hydrateAttachmentReferences(nextContent, onResolveAttachment);
        editor?.commands.setContent(hydrated, { emitUpdate: false });
        lastAppliedContentRef.current = nextContent;
        onUpdate(note.id, nextContent);
      }
    };

    migrate().catch((err) => console.error("Image migration failed:", err));
    return () => { cancelled = true; };
  }, [editor, note.content, note.id, onResolveAttachment, onSaveAttachment, onUpdate]);

  if (!editor) return null;

  return (
    <div className="relative flex h-full flex-col" aria-busy={isSyncing}>
      {isSyncing && (
        <div className="absolute inset-0 z-20 flex items-start justify-center bg-white/30 pt-16 cursor-wait">
          <span className="rounded bg-gray-800 px-3 py-1.5 text-xs text-white shadow">
            同步中，编辑暂时锁定
          </span>
        </div>
      )}
      <Toolbar editor={editor} note={note} onInsertImage={addImageFromFile} />

      <div className="flex-1 overflow-y-auto">
        <EditorContent editor={editor} />
      </div>

      <div className="flex items-center justify-between border-t border-gray-100 px-8 py-2 text-xs text-gray-400">
        <span>
          {new Date(note.updated_at).toLocaleString("zh-CN", {
            month: "short",
            day: "numeric",
            hour: "2-digit",
            minute: "2-digit",
          })}
        </span>
        <span className={saveStatus === "error" ? "text-red-500" : ""}>
          {formatSaveStatus(saveStatus, errorMessage)}
        </span>
        <button onClick={onOpenHistory} className="hover:text-gray-600" title="历史版本">
          v{note.version}
        </button>
      </div>
    </div>
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
