import { useCallback, useEffect, useRef } from "react";
import Image from "@tiptap/extension-image";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Highlight from "@tiptap/extension-highlight";
import Typography from "@tiptap/extension-typography";
import { Markdown } from "@tiptap/markdown";
import StarterKit from "@tiptap/starter-kit";
import { useEditor, EditorContent } from "@tiptap/react";
import { FindReplacePanel, Toolbar, ToolbarButton, InlineMarkdownMarkRules, compressImageToDataUrl, formatSaveStatus, useFindReplace } from "@ui/index";
import { Search } from "lucide-react";
import type { Note, SaveStatus } from "@/types";
import { attachmentsApi } from "@/api/client";

const CloudImage = Image.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      attachmentId: {
        default: null,
        parseHTML: (element) => element.getAttribute("data-attachment-id") || element.getAttribute("src")?.match(/^attachment:\/\/(.+)$/)?.[1] || null,
        renderHTML: (attributes) => attributes.attachmentId ? { "data-attachment-id": attributes.attachmentId } : {},
      },
    };
  },
});

interface NoteEditorProps {
  note: Note;
  onUpdate: (id: string, content: string) => void;
  saveStatus: SaveStatus;
  errorMessage: string | null;
  onOpenHistory?: () => void;
  isSyncing?: boolean;
}

export function NoteEditor({
  note,
  onUpdate,
  saveStatus,
  errorMessage,
  onOpenHistory,
  isSyncing,
}: NoteEditorProps) {
  const noteIdRef = useRef(note.id);
  const onUpdateRef = useRef(onUpdate);
  const markEditorChangedRef = useRef<() => void>(() => {});
  const lastAppliedContentRef = useRef(note.content || "");
  const isApplyingExternalContentRef = useRef(note.content.includes("attachment://"));
  const objectUrlsRef = useRef<string[]>([]);

  useEffect(() => {
    noteIdRef.current = note.id;
    onUpdateRef.current = onUpdate;
  }, [note.id, onUpdate]);

  const handleImageInsert = useCallback(async (file: File) => {
    try {
      const dataUrl = await compressImageToDataUrl(file);
      const { bytes, mimeType } = decodeDataUrl(dataUrl);
      const id = await sha256(bytes);
      await attachmentsApi.upload(id, bytes, mimeType);
      editorRef.current?.chain().focus().setImage({ src: dataUrl, alt: file.name, attachmentId: id } as any).run();
    } catch (err) {
      console.error("Image insert failed:", err);
    }
  }, []);

  const addImageFromFile = useCallback(() => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/*";
    input.onchange = async (e: any) => {
      const file = e.target.files?.[0];
      if (file) await handleImageInsert(file);
    };
    input.click();
  }, [handleImageInsert]);

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: { levels: [1, 2, 3] },
        codeBlock: { HTMLAttributes: { class: "language-plaintext" } },
      }),
      CloudImage.configure({
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
      const html = serializeAttachments(editor.getHTML());
      lastAppliedContentRef.current = html;
      markEditorChangedRef.current();
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

  const editorRef = useRef<any>(null);
  useEffect(() => {
    editorRef.current = editor;
  }, [editor]);

  const findReplace = useFindReplace(editor, note.id);

  useEffect(() => {
    markEditorChangedRef.current = findReplace.markEditorChanged;
  }, [findReplace.markEditorChanged]);

  useEffect(() => {
    editor?.setEditable(!isSyncing);
  }, [editor, isSyncing]);

  // Sync content when switching notes
  useEffect(() => {
    if (!editor) return;
    const nextContent = note.content || "";
    let cancelled = false;
    isApplyingExternalContentRef.current = true;
    void (async () => {
      try {
        const hydrated = await hydrateAttachments(nextContent, objectUrlsRef.current);
        if (cancelled) return;
        if (nextContent !== lastAppliedContentRef.current || editor.isEmpty) {
          editor.commands.setContent(hydrated, { emitUpdate: false });
          lastAppliedContentRef.current = nextContent;
          markEditorChangedRef.current();
        }
      } finally {
        if (!cancelled) isApplyingExternalContentRef.current = false;
      }
    })().catch((error) => console.error("Attachment hydration failed", error));
    return () => { cancelled = true; };
  }, [editor, note.id, note.content]);

  useEffect(() => () => {
    for (const url of objectUrlsRef.current) URL.revokeObjectURL(url);
  }, []);

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
      <Toolbar
        editor={editor}
        note={note}
        onInsertImage={addImageFromFile}
        extraActions={
          <ToolbarButton
            onClick={() => findReplace.setVisible((value) => !value)}
            active={findReplace.visible}
            title="查找替换"
          >
            <Search className="h-4 w-4" />
          </ToolbarButton>
        }
      />

      {findReplace.visible && <FindReplacePanel controls={findReplace} />}

      <div className="flex-1 overflow-y-auto" onDoubleClick={() => editor.chain().focus("end").run()}>
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
        {onOpenHistory ? (
          <button type="button" onClick={onOpenHistory} className="hover:text-gray-600" title="历史版本" aria-label="打开历史版本">
            历史版本
          </button>
        ) : (
          <span>历史版本</span>
        )}
      </div>
    </div>
  );
}

// ── Web-specific attachment helpers (HTTP API) ──

function decodeDataUrl(dataUrl: string): { bytes: Uint8Array; mimeType: string } {
  const [header, payload] = dataUrl.split(",", 2);
  if (!header || !payload) throw new Error("Invalid image data");
  const binary = atob(payload);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return { bytes, mimeType: header.slice(5).split(";", 1)[0] || "application/octet-stream" };
}

async function sha256(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes as BufferSource);
  return Array.from(new Uint8Array(digest), (value) => value.toString(16).padStart(2, "0")).join("");
}

function serializeAttachments(content: string): string {
  if (!content.includes("data-attachment-id")) return content;
  const document = new DOMParser().parseFromString(content, "text/html");
  for (const image of document.querySelectorAll<HTMLImageElement>("img[data-attachment-id]")) {
    const id = image.dataset.attachmentId;
    if (id) image.src = `attachment://${id}`;
  }
  return document.body.innerHTML;
}

async function hydrateAttachments(content: string, objectUrls: string[]): Promise<string> {
  for (const url of objectUrls.splice(0)) URL.revokeObjectURL(url);
  if (!content.includes("attachment://")) return content;
  const document = new DOMParser().parseFromString(content, "text/html");
  const images = Array.from(document.querySelectorAll<HTMLImageElement>("img[src^='attachment://']"));
  await Promise.all(images.map(async (image) => {
    const id = image.src.slice("attachment://".length);
    const blob = await attachmentsApi.download(id);
    const url = URL.createObjectURL(blob);
    objectUrls.push(url);
    image.src = url;
    image.dataset.attachmentId = id;
  }));
  return document.body.innerHTML;
}
