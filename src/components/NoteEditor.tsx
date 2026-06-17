import React, { useCallback, useEffect, useRef, useState } from "react";
import { useEditor, EditorContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Image from "@tiptap/extension-image";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Highlight from "@tiptap/extension-highlight";
import Typography from "@tiptap/extension-typography";
import { Markdown } from "@tiptap/markdown";
import type { Note, SaveStatus } from "@/types";

type EditorViewMode = "edit" | "source" | "preview";

interface NoteEditorProps {
  note: Note;
  onUpdate: (id: string, content: string) => void;
  onSaveAttachment: (dataUrl: string, filename: string) => Promise<string>;
  onOpenHistory: () => void;
  saveStatus: SaveStatus;
  errorMessage: string | null;
}

export function NoteEditor({
  note,
  onUpdate,
  onSaveAttachment,
  onOpenHistory,
  saveStatus,
  errorMessage,
}: NoteEditorProps) {
  const noteIdRef = useRef(note.id);
  const onUpdateRef = useRef(onUpdate);
  const editorRef = useRef<any>(null);
  const migratedNotesRef = useRef<Set<string>>(new Set());
  const [viewMode, setViewMode] = useState<EditorViewMode>("edit");
  const [markdownSource, setMarkdownSource] = useState("");

  useEffect(() => {
    noteIdRef.current = note.id;
    onUpdateRef.current = onUpdate;
  }, [note.id, onUpdate]);

  const handleImageInsert = useCallback(async (file: File) => {
    try {
      const dataUrl = await compressImageToDataUrl(file);
      const src = await onSaveAttachment(dataUrl, file.name);
      editorRef.current?.chain().focus().setImage({ src, alt: file.name }).run();
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
      Image.configure({
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
      Markdown.configure({
        indentation: {
          style: "space",
          size: 2,
        },
      }),
    ],
    content: note.content || "",
    onUpdate: ({ editor }) => {
      const html = editor.getHTML();
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
    editor?.setEditable(viewMode !== "preview");
    if (editor && viewMode === "source") {
      setMarkdownSource(editor.getMarkdown());
    }
  }, [editor, viewMode]);

  // Update editor content when note changes
  useEffect(() => {
    if (editor && note.content !== editor.getHTML()) {
      editor.commands.setContent(note.content || "", { emitUpdate: false });
      if (viewMode === "source") {
        setMarkdownSource(editor.getMarkdown());
      }
    }
  }, [editor, note.id, note.content, viewMode]);

  useEffect(() => {
    if (!note.content.includes("data:image/") || migratedNotesRef.current.has(note.id)) return;
    migratedNotesRef.current.add(note.id);

    let cancelled = false;
    const migrate = async () => {
      const nextContent = await migrateDataUrlImages(note.content, onSaveAttachment);
      if (!cancelled && nextContent !== note.content) {
        editor?.commands.setContent(nextContent, { emitUpdate: false });
        onUpdate(note.id, nextContent);
      }
    };

    migrate().catch((err) => console.error("Image migration failed:", err));
    return () => {
      cancelled = true;
    };
  }, [editor, note.content, note.id, onSaveAttachment, onUpdate]);

  if (!editor) return null;

  const handleMarkdownSourceChange = (value: string) => {
    setMarkdownSource(value);
    editor.commands.setContent(value, {
      contentType: "markdown",
      emitUpdate: false,
    });
    onUpdateRef.current(noteIdRef.current, editor.getHTML());
  };

  return (
    <div className="flex flex-col h-full">
      {/* Toolbar */}
      <Toolbar
        editor={editor}
        note={note}
        onSaveAttachment={onSaveAttachment}
        viewMode={viewMode}
        onViewModeChange={setViewMode}
      />

      {/* Editor Content */}
      <div className="flex-1 overflow-y-auto">
        {viewMode === "source" ? (
          <textarea
            value={markdownSource}
            onChange={(event) => handleMarkdownSourceChange(event.target.value)}
            className="h-full min-h-full w-full resize-none border-0 bg-gray-950 px-8 py-6 font-mono text-sm leading-6 text-gray-100 outline-none"
            spellCheck={false}
          />
        ) : (
          <EditorContent editor={editor} />
        )}
      </div>

      {/* Status Bar */}
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

// Toolbar Component
function Toolbar({
  editor,
  note,
  onSaveAttachment,
  viewMode,
  onViewModeChange,
}: {
  editor: any;
  note: Note;
  onSaveAttachment: (dataUrl: string, filename: string) => Promise<string>;
  viewMode: EditorViewMode;
  onViewModeChange: (mode: EditorViewMode) => void;
}) {
  const editingDisabled = viewMode !== "edit";

  const addImage = useCallback(() => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/*";
    input.onchange = async (e: any) => {
      const file = e.target.files?.[0];
      if (file) {
        const dataUrl = await compressImageToDataUrl(file);
        const src = await onSaveAttachment(dataUrl, file.name);
        editor.chain().focus().setImage({ src, alt: file.name }).run();
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
    const markdown = editor.getMarkdown();
    await navigator.clipboard.writeText(markdown);
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
        disabled={editingDisabled}
        title="标题 1"
      >
        H1
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
        active={editor.isActive("heading", { level: 2 })}
        disabled={editingDisabled}
        title="标题 2"
      >
        H2
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}
        active={editor.isActive("heading", { level: 3 })}
        disabled={editingDisabled}
        title="标题 3"
      >
        H3
      </ToolbarButton>

      <div className="w-px h-5 bg-gray-200 mx-1" />

      <ToolbarButton
        onClick={() => editor.chain().focus().toggleBold().run()}
        active={editor.isActive("bold")}
        disabled={editingDisabled}
        title="粗体 (Ctrl+B)"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={3} d="M6 4h8a4 4 0 014 4 4 4 0 01-4 4H6z M6 12h9a4 4 0 014 4 4 4 0 01-4 4H6z" />
        </svg>
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleItalic().run()}
        active={editor.isActive("italic")}
        disabled={editingDisabled}
        title="斜体 (Ctrl+I)"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 4h4m-2 0l-4 16m-2 0h4" />
        </svg>
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleStrike().run()}
        active={editor.isActive("strike")}
        disabled={editingDisabled}
        title="删除线"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 12h12M8 6h8a2 2 0 010 4H8m0 8h8a2 2 0 000-4" />
        </svg>
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleHighlight().run()}
        active={editor.isActive("highlight")}
        disabled={editingDisabled}
        title="高亮"
      >
        <svg className="w-4 h-4" viewBox="0 0 24 24" fill="currentColor">
          <path d="M15.24 2.79a1 1 0 00-1.41.17L4.6 15.36a1 1 0 00-.17.42L3.8 19.5a.5.5 0 00.6.6l3.72-.63a1 1 0 00.42-.17L19.37 8.93a1 1 0 00.17-1.41l-4.22-4.73z" />
        </svg>
      </ToolbarButton>

      <div className="w-px h-5 bg-gray-200 mx-1" />

      <ToolbarButton
        onClick={() => editor.chain().focus().toggleBulletList().run()}
        active={editor.isActive("bulletList")}
        disabled={editingDisabled}
        title="无序列表"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
        </svg>
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleOrderedList().run()}
        active={editor.isActive("orderedList")}
        disabled={editingDisabled}
        title="有序列表"
      >
        <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 8h10M7 12h10M7 16h10M3 7v2M3 11v2M3 15v2" />
        </svg>
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleTaskList().run()}
        active={editor.isActive("taskList")}
        disabled={editingDisabled}
        title="任务列表"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4" />
        </svg>
      </ToolbarButton>

      <div className="w-px h-5 bg-gray-200 mx-1" />

      <ToolbarButton
        onClick={() => editor.chain().focus().toggleBlockquote().run()}
        active={editor.isActive("blockquote")}
        disabled={editingDisabled}
        title="引用"
      >
        <svg className="w-4 h-4" viewBox="0 0 24 24" fill="currentColor">
          <path d="M4.583 17.321C3.553 16.227 3 15 3 13.011c0-3.5 2.457-6.637 6.03-8.188l.893 1.378c-3.335 1.804-3.987 4.145-4.247 5.621.537-.278 1.24-.375 1.929-.311 1.804.167 3.226 1.648 3.226 3.489a3.5 3.5 0 01-3.5 3.5c-1.073 0-2.099-.49-2.748-1.179zm10 0C13.553 16.227 13 15 13 13.011c0-3.5 2.457-6.637 6.03-8.188l.893 1.378c-3.335 1.804-3.987 4.145-4.247 5.621.537-.278 1.24-.375 1.929-.311 1.804.167 3.226 1.648 3.226 3.489a3.5 3.5 0 01-3.5 3.5c-1.073 0-2.099-.49-2.748-1.179z" />
        </svg>
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleCodeBlock().run()}
        active={editor.isActive("codeBlock")}
        disabled={editingDisabled}
        title="代码块"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
        </svg>
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().setHorizontalRule().run()}
        disabled={editingDisabled}
        title="分割线"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 12h16" />
        </svg>
      </ToolbarButton>

      <div className="w-px h-5 bg-gray-200 mx-1" />

      <ToolbarButton onClick={addImage} disabled={editingDisabled} title="插入图片">
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
      </ToolbarButton>

      <div className="w-px h-5 bg-gray-200 mx-1" />

      <ToolbarButton onClick={importMarkdown} title="导入 Markdown">
        MD
      </ToolbarButton>
      <ToolbarButton onClick={copyMarkdown} title="复制为 Markdown">
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16h8M8 12h8m-7 8h6a2 2 0 002-2V7.5L13.5 4H9a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
      </ToolbarButton>
      <ToolbarButton onClick={exportMarkdown} title="导出 Markdown">
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v10m0 0l-3-3m3 3l3-3M5 20h14" />
        </svg>
      </ToolbarButton>

      <div className="w-px h-5 bg-gray-200 mx-1" />

      <ToolbarButton
        onClick={() => onViewModeChange("edit")}
        active={viewMode === "edit"}
        title="富文本编辑"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 20h16M6 16l9.5-9.5a2.1 2.1 0 013 3L9 19H6v-3z" />
        </svg>
      </ToolbarButton>
      <ToolbarButton
        onClick={() => onViewModeChange("source")}
        active={viewMode === "source"}
        title="Markdown 源码"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 9l-4 3 4 3m8-6l4 3-4 3M14 5l-4 14" />
        </svg>
      </ToolbarButton>
      <ToolbarButton
        onClick={() => onViewModeChange("preview")}
        active={viewMode === "preview"}
        title="预览"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2 12s3.5-6 10-6 10 6 10 6-3.5 6-10 6S2 12 2 12z" />
          <circle cx="12" cy="12" r="3" />
        </svg>
      </ToolbarButton>
    </div>
  );
}

function ToolbarButton({
  children,
  onClick,
  active,
  title,
  disabled,
}: {
  children: React.ReactNode;
  onClick: () => void;
  active?: boolean;
  title?: string;
  disabled?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      title={title}
      disabled={disabled}
      className={`w-7 h-7 flex items-center justify-center rounded text-xs font-medium transition-colors ${
        disabled
          ? "cursor-not-allowed text-gray-300"
          : active
          ? "bg-blue-100 text-blue-700"
          : "text-gray-600 hover:bg-gray-100 hover:text-gray-800"
      }`}
    >
      {children}
    </button>
  );
}

// Image compression utility
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
  saveAttachment: (dataUrl: string, filename: string) => Promise<string>
): Promise<string> {
  const doc = new DOMParser().parseFromString(`<main>${content}</main>`, "text/html");
  const images = Array.from(doc.querySelectorAll<HTMLImageElement>("img[src^='data:image/']"));

  for (const [index, image] of images.entries()) {
    const src = image.getAttribute("src");
    if (!src) continue;
    const nextSrc = await saveAttachment(src, image.getAttribute("alt") || `image-${index + 1}.webp`);
    image.setAttribute("src", nextSrc);
  }

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
