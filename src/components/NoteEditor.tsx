import React, { useCallback, useEffect, useRef } from "react";
import { Extension, InputRule } from "@tiptap/core";
import type { MarkType } from "@tiptap/pm/model";
import { useEditor, EditorContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Image from "@tiptap/extension-image";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Highlight from "@tiptap/extension-highlight";
import Typography from "@tiptap/extension-typography";
import { Markdown } from "@tiptap/markdown";
import {
  Bold,
  Italic,
  Strikethrough,
  Highlighter,
  List,
  ListOrdered,
  ListChecks,
  Quote,
  Code,
  Minus,
  ImagePlus,
  FileDown,
  FileUp,
  Copy,
} from "lucide-react";
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

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: {
          levels: [1, 2, 3],
        },
        codeBlock: {
          HTMLAttributes: {
            class: "language-plaintext",
          },
        },
      }),
      AttachmentImage.configure({
        inline: false,
        allowBase64: true,
        HTMLAttributes: {
          class: "rounded-lg max-w-full",
        },
      }),
      Placeholder.configure({
        placeholder: "开始记录你的想法...",
      }),
      TaskList,
      TaskItem.configure({
        nested: true,
      }),
      Highlight.configure({
        multicolor: false,
      }),
      Typography,
      InlineMarkdownMarkRules,
      Markdown.configure({
        indentation: {
          style: "space",
          size: 2,
        },
      }),
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
              if (file) {
                handleImageInsert(file);
              }
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
    return () => {
      cancelled = true;
    };
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
    return () => {
      cancelled = true;
    };
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
      <Toolbar editor={editor} note={note} onSaveAttachment={onSaveAttachment} />

      <div className="flex-1 overflow-y-auto">
        <EditorContent editor={editor} />
      </div>

      <div className="px-8 py-2 border-t border-gray-100 flex items-center justify-between text-xs text-gray-400">
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

function Toolbar({
  editor,
  note,
  onSaveAttachment,
}: {
  editor: any;
  note: Note;
  onSaveAttachment: (dataUrl: string, filename: string) => Promise<Attachment>;
}) {
  const addImage = useCallback(() => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/*";
    input.onchange = async (e: any) => {
      const file = e.target.files?.[0];
      if (file) {
        const dataUrl = await compressImageToDataUrl(file);
        const attachment = await onSaveAttachment(dataUrl, file.name);
        editor.chain().focus().setImage({
          src: attachment.path,
          alt: file.name,
          attachmentId: attachment.id,
        } as any).run();
      }
    };
    input.click();
  }, [editor, onSaveAttachment]);

  const importMarkdown = useCallback(() => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".md,.markdown,text/markdown,text/plain";
    input.onchange = async (e: any) => {
      const file = e.target.files?.[0];
      if (!file) return;
      const markdown = await file.text();
      editor.commands.setContent(markdown, { contentType: "markdown" });
    };
    input.click();
  }, [editor]);

  const copyMarkdown = useCallback(async () => {
    await navigator.clipboard.writeText(editor.getMarkdown());
  }, [editor]);

  const exportMarkdown = useCallback(() => {
    const markdown = editor.getMarkdown();
    const blob = new Blob([markdown], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `${sanitizeFilename(note.title || "QuickNote")}.md`;
    link.click();
    URL.revokeObjectURL(url);
  }, [editor, note.title]);

  return (
    <div className="px-8 py-2 border-b border-gray-100 flex items-center gap-1 flex-wrap">
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleHeading({ level: 1 }).run()}
        active={editor.isActive("heading", { level: 1 })}
        title="标题 1"
      >
        H1
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
        active={editor.isActive("heading", { level: 2 })}
        title="标题 2"
      >
        H2
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}
        active={editor.isActive("heading", { level: 3 })}
        title="标题 3"
      >
        H3
      </ToolbarButton>

      <ToolbarDivider />

      <ToolbarButton
        onClick={() => editor.chain().focus().toggleBold().run()}
        active={editor.isActive("bold")}
        title="粗体 (Ctrl+B)"
      >
        <Bold className="w-4 h-4" />
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleItalic().run()}
        active={editor.isActive("italic")}
        title="斜体 (Ctrl+I)"
      >
        <Italic className="w-4 h-4" />
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleStrike().run()}
        active={editor.isActive("strike")}
        title="删除线"
      >
        <Strikethrough className="w-4 h-4" />
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleHighlight().run()}
        active={editor.isActive("highlight")}
        title="高亮"
      >
        <Highlighter className="w-4 h-4" />
      </ToolbarButton>

      <ToolbarDivider />

      <ToolbarButton
        onClick={() => editor.chain().focus().toggleBulletList().run()}
        active={editor.isActive("bulletList")}
        title="无序列表"
      >
        <List className="w-4 h-4" />
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleOrderedList().run()}
        active={editor.isActive("orderedList")}
        title="有序列表"
      >
        <ListOrdered className="w-4 h-4" />
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleTaskList().run()}
        active={editor.isActive("taskList")}
        title="任务列表"
      >
        <ListChecks className="w-4 h-4" />
      </ToolbarButton>

      <ToolbarDivider />

      <ToolbarButton
        onClick={() => editor.chain().focus().toggleBlockquote().run()}
        active={editor.isActive("blockquote")}
        title="引用"
      >
        <Quote className="w-4 h-4" />
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleCodeBlock().run()}
        active={editor.isActive("codeBlock")}
        title="代码块"
      >
        <Code className="w-4 h-4" />
      </ToolbarButton>
      <ToolbarButton onClick={() => editor.chain().focus().setHorizontalRule().run()} title="分割线">
        <Minus className="w-4 h-4" />
      </ToolbarButton>

      <ToolbarDivider />

      <ToolbarButton onClick={addImage} title="插入图片">
        <ImagePlus className="w-4 h-4" />
      </ToolbarButton>

      <ToolbarDivider />

      <ToolbarButton onClick={importMarkdown} title="导入 Markdown">
        <FileUp className="w-4 h-4" />
      </ToolbarButton>
      <ToolbarButton onClick={copyMarkdown} title="复制为 Markdown">
        <Copy className="w-4 h-4" />
      </ToolbarButton>
      <ToolbarButton onClick={exportMarkdown} title="导出 Markdown">
        <FileDown className="w-4 h-4" />
      </ToolbarButton>
    </div>
  );
}

function ToolbarDivider() {
  return <div className="w-px h-5 bg-gray-200 mx-1" />;
}

function ToolbarButton({
  children,
  onClick,
  active,
  title,
}: {
  children: React.ReactNode;
  onClick: () => void;
  active?: boolean;
  title?: string;
}) {
  return (
    <button
      onClick={onClick}
      title={title}
      className={`w-7 h-7 flex items-center justify-center rounded text-xs font-medium transition-colors ${
        active ? "bg-blue-100 text-blue-700" : "text-gray-600 hover:bg-gray-100 hover:text-gray-800"
      }`}
    >
      {children}
    </button>
  );
}

async function compressImageToDataUrl(file: File, maxWidth = 1920, quality = 0.82): Promise<string> {
  return new Promise((resolve, reject) => {
    const img = new window.Image();
    const objectUrl = URL.createObjectURL(file);

    img.onload = () => {
      const canvas = document.createElement("canvas");
      let { width, height } = img;

      if (width > maxWidth) {
        height = (height * maxWidth) / width;
        width = maxWidth;
      }

      canvas.width = width;
      canvas.height = height;

      const ctx = canvas.getContext("2d")!;
      ctx.drawImage(img, 0, 0, width, height);

      const outputType = file.type === "image/png" ? "image/png" : "image/webp";
      URL.revokeObjectURL(objectUrl);
      resolve(canvas.toDataURL(outputType, quality));
    };
    img.onerror = () => {
      URL.revokeObjectURL(objectUrl);
      reject(new Error("Image decode failed"));
    };
    img.src = objectUrl;
  });
}

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

function formatSaveStatus(status: SaveStatus, errorMessage: string | null): string {
  if (status === "saving") return "保存中";
  if (status === "retrying") return "重试保存中";
  if (status === "saved") return "已保存";
  if (status === "error") return errorMessage ? `保存失败：${errorMessage}` : "保存失败";
  return "未修改";
}

function sanitizeFilename(value: string): string {
  return value.replace(/[\\/:*?"<>|]/g, "_").slice(0, 80) || "QuickNote";
}

const InlineMarkdownMarkRules = Extension.create({
  name: "inlineMarkdownMarkRules",

  addInputRules() {
    return INLINE_MARK_RULES.flatMap(({ markName, delimiter }) => {
      const type = this.editor.schema.marks[markName];
      return type ? [createDelimitedMarkRule(type, delimiter)] : [];
    });
  },
});

const INLINE_MARK_RULES = [
  { markName: "bold", delimiter: "**" },
  { markName: "bold", delimiter: "__" },
  { markName: "strike", delimiter: "~~" },
  { markName: "highlight", delimiter: "==" },
  { markName: "code", delimiter: "`" },
  { markName: "italic", delimiter: "*" },
  { markName: "italic", delimiter: "_" },
] as const;

function createDelimitedMarkRule(type: MarkType, delimiter: string) {
  return new InputRule({
    find: (text) => {
      const match = findDelimitedMark(text, delimiter);
      if (!match) return null;

      return {
        index: match.openStart,
        text: text.slice(match.openStart),
        data: {
          content: match.content,
          trailing: match.trailing,
        },
      };
    },
    handler: ({ state, range, match }) => {
      const content = match.data?.content as string | undefined;
      const trailing = (match.data?.trailing as string | undefined) || "";
      if (!content) return null;

      const { tr } = state;
      const trailingLength = trailing.length;
      const contentStart = range.from + delimiter.length;
      const contentEnd = contentStart + content.length;
      const closeStart = contentEnd;
      const closeEnd = range.to - trailingLength;

      tr.delete(closeStart, closeEnd);
      tr.delete(range.from, range.from + delimiter.length);
      tr.addMark(range.from, range.from + content.length, type.create());
      tr.removeStoredMark(type);
    },
  });
}

function findDelimitedMark(text: string, delimiter: string) {
  const trailing = text.endsWith(" ") ? " " : "";
  const closeEnd = text.length - trailing.length;
  const closeStart = closeEnd - delimiter.length;

  if (closeStart <= delimiter.length || text.slice(closeStart, closeEnd) !== delimiter) {
    return null;
  }

  const openStart = text.lastIndexOf(delimiter, closeStart - 1);
  if (openStart < 0) return null;

  if (delimiter.length === 1) {
    if (text[openStart - 1] === delimiter || text[closeEnd] === delimiter) {
      return null;
    }
  }

  const contentStart = openStart + delimiter.length;
  const content = text.slice(contentStart, closeStart);
  if (!content || content.trim() !== content) return null;

  return {
    openStart,
    content,
    trailing,
  };
}
