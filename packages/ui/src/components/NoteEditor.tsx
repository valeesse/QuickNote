import { useCallback, useEffect, useMemo, useRef } from "react";
import Image from "@tiptap/extension-image";
import Placeholder from "@tiptap/extension-placeholder";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import Highlight from "@tiptap/extension-highlight";
import Typography from "@tiptap/extension-typography";
import { Collaboration } from "@tiptap/extension-collaboration";
import { Markdown } from "@tiptap/markdown";
import StarterKit from "@tiptap/starter-kit";
import { EditorContent, useEditor } from "@tiptap/react";
import * as Y from "yjs";
import type { Note, SaveStatus, TagSummary } from "@contracts";
import { EditorShell } from "./EditorShell";
import { InlineMarkdownMarkRules } from "./MarkdownRules";
import { createAttachmentImageExtension } from "../editor/attachments";
import { useAttachmentEditorBridge, type InsertedEditorImage } from "../editor/useAttachmentEditorBridge";
import { useYjsDoc, type YjsDocState } from "../editor/useYjsDoc";

export interface SharedNoteEditorProps {
  note: Note;
  tags: TagSummary[];
  saveStatus: SaveStatus;
  errorMessage: string | null;
  isSyncing?: boolean;
  websocketUrl?: string | null;
  onUpdate: (id: string, content: string, yjsState?: number[]) => void;
  onUpdateTags: (noteId: string, tags: string[]) => void;
  onOpenHistory?: () => void;
  saveImage: (file: File, dataUrl: string) => Promise<InsertedEditorImage>;
  resolveImageSrc: (attachmentId: string) => Promise<string>;
  serializeContent: (html: string) => string;
  hydrateContent: (content: string) => Promise<string>;
  shouldMigrateContent?: (note: Note) => boolean;
  migrateContent?: (content: string) => Promise<string>;
}

export function SharedNoteEditor(props: SharedNoteEditorProps) {
  const yjs = useYjsDoc({
    noteId: props.note.id,
    state: props.note.yjs_state,
    stateVersion: props.note.yjs_state_version,
    websocketUrl: props.websocketUrl,
    collaborative: true,
  });
  if (!yjs.isReady) return null;
  return <ReadyNoteEditor props={props} yjs={yjs} />;
}

function ReadyNoteEditor({ props, yjs }: { props: SharedNoteEditorProps; yjs: YjsDocState }) {
  const {
    note, tags, saveStatus, errorMessage, isSyncing, websocketUrl,
    onUpdate, onUpdateTags, onOpenHistory, saveImage, serializeContent,
    hydrateContent, shouldMigrateContent, migrateContent, resolveImageSrc,
  } = props;
  const AttachmentImage = useMemo(
    () => createAttachmentImageExtension(Image, resolveImageSrc),
    [resolveImageSrc],
  );
  const importedContentRef = useRef("");
  const handleUpdate = useCallback((id: string, html: string) => {
    const content = serializeContent(html);
    importedContentRef.current = content;
    if (websocketUrl) {
      yjs.sendProjection(content);
      return;
    }
    onUpdate(id, content, Array.from(Y.encodeStateAsUpdate(yjs.doc)));
  }, [onUpdate, serializeContent, websocketUrl, yjs.doc, yjs.sendProjection]);
  const bridge = useAttachmentEditorBridge({
    note,
    isSyncing,
    onUpdate: handleUpdate,
    managedContent: true,
    serializeContent,
    hydrateContent,
    saveImage,
    shouldMigrateContent,
    migrateContent,
  });
  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: { levels: [1, 2, 3] },
        codeBlock: { HTMLAttributes: { class: "language-plaintext" } },
        undoRedo: false,
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
      Collaboration.configure({ document: yjs.doc, field: "prosemirror" }),
    ],
    content: "",
    onUpdate: ({ editor: activeEditor }) => bridge.handleEditorUpdate(activeEditor),
    editorProps: bridge.editorProps,
  });

  useEffect(() => bridge.setEditor(editor), [bridge.setEditor, editor]);
  useEffect(() => {
    if (!editor || !yjs.isReady || !yjs.shouldBootstrap || !note.content) return;
    const isLegacyRefresh = !note.yjs_state?.length && importedContentRef.current !== note.content;
    if (!editor.isEmpty && !isLegacyRefresh) return;
    let cancelled = false;
    void (async () => shouldMigrateContent?.(note) && migrateContent
      ? migrateContent(note.content)
      : note.content)().then((content: string) => {
      if (!cancelled && !editor.isDestroyed) {
        importedContentRef.current = note.content;
        editor.commands.setContent(content);
      }
    });
    return () => { cancelled = true; };
  }, [editor, migrateContent, note, shouldMigrateContent, websocketUrl, yjs.isReady, yjs.shouldBootstrap]);

  if (!editor) return null;
  const effectiveStatus = websocketUrl
    ? yjs.delivery
    : saveStatus;
  return (
    <EditorShell
      editor={editor}
      note={note}
      saveStatus={effectiveStatus}
      errorMessage={errorMessage}
      isSyncing={isSyncing}
      onInsertImage={bridge.addImageFromFile}
      findReplace={bridge.findReplace}
      onOpenHistory={onOpenHistory}
      onUpdateTags={(nextTags) => onUpdateTags(note.id, nextTags)}
      tagSuggestions={tags}
    >
      <EditorContent editor={editor} />
    </EditorShell>
  );
}
