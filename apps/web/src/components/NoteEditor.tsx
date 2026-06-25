import { useCallback, useEffect, useRef } from "react";
import Image from "@tiptap/extension-image";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Highlight from "@tiptap/extension-highlight";
import Typography from "@tiptap/extension-typography";
import { Collaboration } from "@tiptap/extension-collaboration";
import { Markdown } from "@tiptap/markdown";
import StarterKit from "@tiptap/starter-kit";
import { useEditor, EditorContent } from "@tiptap/react";
import { EditorShell, InlineMarkdownMarkRules, createAttachmentImageExtension, useAttachmentEditorBridge, useYjsDoc } from "@ui/index";
import type { Note, SaveStatus } from "@/types";
import { attachmentsApi, getBaseUrl } from "@/api/client";

const CloudImage = createAttachmentImageExtension(Image);

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
  const objectUrlsRef = useRef<string[]>([]);
  const yjsDoc = useYjsDoc({
    noteId: note.id,
    state: note.yjs_state,
    stateVersion: note.yjs_state_version,
    websocketUrl: getCollabWebSocketUrl(note.id, note.yjs_state),
  });
  const bridge = useAttachmentEditorBridge({
    note,
    isSyncing,
    onUpdate,
    managedContent: yjsDoc.isCollaborative,
    serializeContent: serializeAttachments,
    hydrateContent: useCallback(
      (content: string) => hydrateAttachments(content, objectUrlsRef.current),
      [],
    ),
    saveImage: useCallback(async (file: File, dataUrl: string) => {
      const { bytes, mimeType } = decodeDataUrl(dataUrl);
      const id = await sha256(bytes);
      await attachmentsApi.upload(id, bytes, mimeType);
      return {
        src: dataUrl,
        alt: file.name,
        attachmentId: id,
      };
    }, []),
  });

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: { levels: [1, 2, 3] },
        codeBlock: { HTMLAttributes: { class: "language-plaintext" } },
        undoRedo: yjsDoc.isCollaborative ? false : undefined,
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
      ...(yjsDoc.isCollaborative
        ? [Collaboration.configure({ document: yjsDoc.doc, field: "prosemirror" })]
        : []),
    ],
    content: yjsDoc.isCollaborative || note.content.includes("attachment://") ? "" : note.content || "",
    onUpdate: ({ editor }) => bridge.handleEditorUpdate(editor),
    editorProps: bridge.editorProps,
  });

  useEffect(() => {
    bridge.setEditor(editor);
  }, [bridge.setEditor, editor]);

  useEffect(() => () => {
    for (const url of objectUrlsRef.current) URL.revokeObjectURL(url);
  }, []);

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

// ── Web-specific attachment helpers (HTTP API) ──

function getCollabWebSocketUrl(noteId: string, state?: number[] | null): string | null {
  if (!state?.length) return null;
  const base = getBaseUrl();
  const origin = base ? new URL(base, window.location.origin) : window.location;
  const protocol = origin.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${origin.host}/api/collab/notes/${encodeURIComponent(noteId)}/ws`;
}

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
