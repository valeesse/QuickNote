import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Image from "@tiptap/extension-image";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Highlight from "@tiptap/extension-highlight";
import Typography from "@tiptap/extension-typography";
import { Markdown } from "@tiptap/markdown";
import StarterKit from "@tiptap/starter-kit";
import { useEditor, EditorContent } from "@tiptap/react";
import { Toolbar, ToolbarButton, InlineMarkdownMarkRules, compressImageToDataUrl, formatSaveStatus } from "@ui/index";
import { Check, Regex, Replace, Search, X } from "lucide-react";
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
}

export function NoteEditor({
  note,
  onUpdate,
  saveStatus,
  errorMessage,
}: NoteEditorProps) {
  const noteIdRef = useRef(note.id);
  const onUpdateRef = useRef(onUpdate);
  const lastAppliedContentRef = useRef(note.content || "");
  const objectUrlsRef = useRef<string[]>([]);
  const [findQuery, setFindQuery] = useState("");
  const [replaceQuery, setReplaceQuery] = useState("");
  const [useRegex, setUseRegex] = useState(false);
  const [currentMatchIndex, setCurrentMatchIndex] = useState(0);
  const [editorRevision, setEditorRevision] = useState(0);
  const [showFindReplace, setShowFindReplace] = useState(false);

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
      const html = serializeAttachments(editor.getHTML());
      lastAppliedContentRef.current = html;
      setEditorRevision((revision) => revision + 1);
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

  const findState = useMemo(
    () => (editor ? collectTextMatches(editor, findQuery, useRegex, editorRevision) : { matches: [], error: null }),
    [editor, editorRevision, findQuery, useRegex],
  );

  useEffect(() => {
    setCurrentMatchIndex(0);
  }, [findQuery, useRegex, note.id]);

  useEffect(() => {
    if (!editor || findState.matches.length === 0) return;
    const match = findState.matches[Math.min(currentMatchIndex, findState.matches.length - 1)];
    if (!match) return;
    editor.chain().focus().setTextSelection({ from: match.from, to: match.to }).run();
  }, [currentMatchIndex, editor, findState.matches]);

  // Sync content when switching notes
  useEffect(() => {
    if (!editor) return;
    const nextContent = note.content || "";
    let cancelled = false;
    void (async () => {
      const hydrated = await hydrateAttachments(nextContent, objectUrlsRef.current);
      if (cancelled) return;
      if (nextContent !== lastAppliedContentRef.current || editor.isEmpty) {
        editor.commands.setContent(hydrated, { emitUpdate: false });
        lastAppliedContentRef.current = nextContent;
        setEditorRevision((revision) => revision + 1);
      }
    })().catch((error) => console.error("Attachment hydration failed", error));
    return () => { cancelled = true; };
  }, [editor, note.id, note.content]);

  useEffect(() => () => {
    for (const url of objectUrlsRef.current) URL.revokeObjectURL(url);
  }, []);

  if (!editor) return null;

  const goToMatch = (direction: 1 | -1) => {
    if (findState.matches.length === 0) return;
    setCurrentMatchIndex((index) => (index + direction + findState.matches.length) % findState.matches.length);
  };

  const replaceCurrent = () => {
    const match = findState.matches[currentMatchIndex];
    if (!match || findState.error) return;
    editor
      .chain()
      .focus()
      .insertContentAt({ from: match.from, to: match.to }, buildReplacement(match.text, findQuery, replaceQuery, useRegex))
      .run();
    setEditorRevision((revision) => revision + 1);
  };

  const replaceAll = () => {
    if (findState.matches.length === 0 || findState.error) return;
    for (const match of [...findState.matches].reverse()) {
      editor
        .chain()
        .insertContentAt({ from: match.from, to: match.to }, buildReplacement(match.text, findQuery, replaceQuery, useRegex))
        .run();
    }
    editor.commands.focus();
    setCurrentMatchIndex(0);
    setEditorRevision((revision) => revision + 1);
  };

  return (
    <div className="relative flex h-full flex-col">
      <Toolbar
        editor={editor}
        note={note}
        onInsertImage={addImageFromFile}
        extraActions={
          <ToolbarButton
            onClick={() => setShowFindReplace((value) => !value)}
            active={showFindReplace}
            title="查找替换"
          >
            <Search className="h-4 w-4" />
          </ToolbarButton>
        }
      />

      {showFindReplace && (
        <div className="absolute right-6 top-12 z-30 w-[min(520px,calc(100vw-2rem))] rounded-lg border border-gray-200 bg-white p-3 text-xs shadow-xl">
          <div className="mb-2 flex items-center justify-between">
            <span className="font-medium text-gray-700">查找替换</span>
            <button type="button" onClick={() => setShowFindReplace(false)} className="focus-ring rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600" title="关闭" aria-label="关闭查找替换">
              <X className="h-3.5 w-3.5" />
            </button>
          </div>
          <div className="grid gap-2">
            <label className="relative">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-gray-400" />
              <input
                value={findQuery}
                onChange={(event) => setFindQuery(event.target.value)}
                placeholder="查找"
                autoFocus
                className={`h-8 w-full rounded-lg border bg-gray-50 pl-8 pr-3 outline-none transition focus:bg-white focus:ring-2 ${
                  findState.error ? "border-red-200 focus:ring-red-100" : "border-gray-200 focus:border-blue-300 focus:ring-blue-100"
                }`}
              />
            </label>
            <label className="relative">
              <Replace className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-gray-400" />
              <input
                value={replaceQuery}
                onChange={(event) => setReplaceQuery(event.target.value)}
                placeholder="替换为"
                className="h-8 w-full rounded-lg border border-gray-200 bg-gray-50 pl-8 pr-3 outline-none transition focus:border-blue-300 focus:bg-white focus:ring-2 focus:ring-blue-100"
              />
            </label>
            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={() => setUseRegex((value) => !value)}
                aria-pressed={useRegex}
                className={`focus-ring flex h-8 items-center gap-1 rounded-lg border px-2 ${useRegex ? "border-blue-200 bg-blue-50 text-blue-700" : "border-gray-200 text-gray-500 hover:bg-gray-50"}`}
                title="使用正则"
              >
                <Regex className="h-3.5 w-3.5" />
                正则
              </button>
              <span className={`min-w-[64px] text-center ${findState.error ? "text-red-500" : "text-gray-400"}`}>
                {findState.error ? "正则错误" : findQuery ? `${findState.matches.length ? currentMatchIndex + 1 : 0}/${findState.matches.length}` : "0/0"}
              </span>
              <button type="button" onClick={() => goToMatch(-1)} disabled={findState.matches.length === 0} className="focus-ring h-8 rounded-lg border border-gray-200 px-2 text-gray-600 hover:bg-gray-50 disabled:opacity-40">上一个</button>
              <button type="button" onClick={() => goToMatch(1)} disabled={findState.matches.length === 0} className="focus-ring h-8 rounded-lg border border-gray-200 px-2 text-gray-600 hover:bg-gray-50 disabled:opacity-40">下一个</button>
              <button type="button" onClick={replaceCurrent} disabled={findState.matches.length === 0 || Boolean(findState.error)} className="focus-ring flex h-8 items-center gap-1 rounded-lg border border-gray-200 px-2 text-gray-600 hover:bg-gray-50 disabled:opacity-40">
                <Check className="h-3.5 w-3.5" />
                替换
              </button>
              <button type="button" onClick={replaceAll} disabled={findState.matches.length === 0 || Boolean(findState.error)} className="focus-ring h-8 rounded-lg bg-gray-900 px-3 font-medium text-white hover:bg-gray-800 disabled:opacity-40">全部</button>
            </div>
          </div>
        </div>
      )}

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
        <span>历史版本</span>
      </div>
    </div>
  );
}

interface TextMatch {
  from: number;
  to: number;
  text: string;
}

function collectTextMatches(
  editor: NonNullable<ReturnType<typeof useEditor>>,
  query: string,
  regex: boolean,
  _revision: number,
): { matches: TextMatch[]; error: string | null } {
  if (!query) return { matches: [], error: null };

  let matcher: RegExp;
  try {
    matcher = regex ? new RegExp(query, "gi") : new RegExp(escapeRegExp(query), "gi");
  } catch (error) {
    return { matches: [], error: getErrorMessage(error) };
  }

  const matches: TextMatch[] = [];
  editor.state.doc.descendants((node: any, pos: number) => {
    if (!node.isText || !node.text) return;
    matcher.lastIndex = 0;
    let match: RegExpExecArray | null;
    while ((match = matcher.exec(node.text))) {
      if (match[0].length === 0) {
        matcher.lastIndex += 1;
        continue;
      }
      matches.push({
        from: pos + match.index,
        to: pos + match.index + match[0].length,
        text: match[0],
      });
    }
  });

  return { matches, error: null };
}

function buildReplacement(text: string, query: string, replacement: string, regex: boolean): string {
  if (!regex) return replacement;
  try {
    return text.replace(new RegExp(query, "i"), replacement);
  } catch {
    return replacement;
  }
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function getErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
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
