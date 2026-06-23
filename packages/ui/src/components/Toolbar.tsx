import React, { useCallback } from "react";
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
import { sanitizeFilename } from "../utils/format";

interface ToolbarProps {
  // biome-ignore lint/suspicious/noExplicitAny: TipTap Editor type is complex
  editor: any;
  note: { title: string };
  onInsertImage: () => void;
}

export function Toolbar({ editor, note, onInsertImage }: ToolbarProps) {
  const importMarkdown = useCallback(() => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".md,.markdown,text/markdown,text/plain";
    input.onchange = async (e: Event) => {
      const file = (e.target as HTMLInputElement).files?.[0];
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
    <div className="flex flex-wrap items-center gap-1 border-b border-gray-100 px-8 py-2">
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
        <Bold className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleItalic().run()}
        active={editor.isActive("italic")}
        title="斜体 (Ctrl+I)"
      >
        <Italic className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleStrike().run()}
        active={editor.isActive("strike")}
        title="删除线"
      >
        <Strikethrough className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleHighlight().run()}
        active={editor.isActive("highlight")}
        title="高亮"
      >
        <Highlighter className="h-4 w-4" />
      </ToolbarButton>

      <ToolbarDivider />

      <ToolbarButton
        onClick={() => editor.chain().focus().toggleBulletList().run()}
        active={editor.isActive("bulletList")}
        title="无序列表"
      >
        <List className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleOrderedList().run()}
        active={editor.isActive("orderedList")}
        title="有序列表"
      >
        <ListOrdered className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleTaskList().run()}
        active={editor.isActive("taskList")}
        title="任务列表"
      >
        <ListChecks className="h-4 w-4" />
      </ToolbarButton>

      <ToolbarDivider />

      <ToolbarButton
        onClick={() => editor.chain().focus().toggleBlockquote().run()}
        active={editor.isActive("blockquote")}
        title="引用"
      >
        <Quote className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton
        onClick={() => editor.chain().focus().toggleCodeBlock().run()}
        active={editor.isActive("codeBlock")}
        title="代码块"
      >
        <Code className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton onClick={() => editor.chain().focus().setHorizontalRule().run()} title="分割线">
        <Minus className="h-4 w-4" />
      </ToolbarButton>

      <ToolbarDivider />

      <ToolbarButton onClick={onInsertImage} title="插入图片">
        <ImagePlus className="h-4 w-4" />
      </ToolbarButton>

      <ToolbarDivider />

      <ToolbarButton onClick={importMarkdown} title="导入 Markdown">
        <FileUp className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton onClick={copyMarkdown} title="复制为 Markdown">
        <Copy className="h-4 w-4" />
      </ToolbarButton>
      <ToolbarButton onClick={exportMarkdown} title="导出 Markdown">
        <FileDown className="h-4 w-4" />
      </ToolbarButton>
    </div>
  );
}

export function ToolbarDivider() {
  return <div className="mx-1 h-5 w-px bg-gray-200" />;
}

export function ToolbarButton({
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
      type="button"
      onClick={onClick}
      title={title}
      aria-label={title}
      aria-pressed={active ?? undefined}
      className={`flex h-7 w-7 items-center justify-center rounded text-xs font-medium transition-colors ${
        active
          ? "bg-blue-100 text-blue-700"
          : "text-gray-600 hover:bg-gray-100 hover:text-gray-800"
      } focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500/40 focus-visible:ring-offset-1`}
    >
      {children}
    </button>
  );
}
