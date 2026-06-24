import { useCallback, useEffect, useRef, useState } from "react";
import { useFindReplace } from "../hooks/useFindReplace";
import { pickImageFile } from "../utils/file";
import { compressImageToDataUrl } from "../utils/image";

export interface InsertedEditorImage {
  src: string;
  alt?: string;
  attachmentId?: string;
}

export interface AttachmentEditorBridgeOptions<NoteLike extends { id: string; content: string }> {
  note: NoteLike;
  isSyncing?: boolean;
  onUpdate: (id: string, content: string) => void;
  serializeContent: (html: string) => string;
  hydrateContent: (content: string) => Promise<string>;
  saveImage: (file: File, dataUrl: string) => Promise<InsertedEditorImage>;
  shouldMigrateContent?: (note: NoteLike) => boolean;
  migrateContent?: (content: string) => Promise<string>;
}

export function useAttachmentEditorBridge<NoteLike extends { id: string; content: string }>({
  note,
  isSyncing,
  onUpdate,
  serializeContent,
  hydrateContent,
  saveImage,
  shouldMigrateContent,
  migrateContent,
}: AttachmentEditorBridgeOptions<NoteLike>) {
  const [editor, setEditor] = useState<any>(null);
  const noteIdRef = useRef(note.id);
  const onUpdateRef = useRef(onUpdate);
  const editorRef = useRef<any>(null);
  const markEditorChangedRef = useRef<() => void>(() => {});
  const lastAppliedContentRef = useRef(note.content || "");
  const isApplyingExternalContentRef = useRef(note.content.includes("attachment://"));
  const migratedNotesRef = useRef<Set<string>>(new Set());
  const findReplace = useFindReplace(editor, note.id);

  useEffect(() => {
    noteIdRef.current = note.id;
    onUpdateRef.current = onUpdate;
  }, [note.id, onUpdate]);

  useEffect(() => {
    editorRef.current = editor;
  }, [editor]);

  useEffect(() => {
    markEditorChangedRef.current = findReplace.markEditorChanged;
  }, [findReplace.markEditorChanged]);

  useEffect(() => {
    editor?.setEditable(!isSyncing);
  }, [editor, isSyncing]);

  const handleEditorUpdate = useCallback((activeEditor: any) => {
    if (isApplyingExternalContentRef.current) return;
    const html = serializeContent(activeEditor.getHTML());
    lastAppliedContentRef.current = html;
    markEditorChangedRef.current();
    onUpdateRef.current(noteIdRef.current, html);
  }, [serializeContent]);

  const insertImageFile = useCallback(async (file: File) => {
    try {
      const dataUrl = await compressImageToDataUrl(file);
      const image = await saveImage(file, dataUrl);
      editorRef.current?.chain().focus().setImage({
        src: image.src,
        alt: image.alt ?? file.name,
        attachmentId: image.attachmentId,
      } as any).run();
    } catch (error) {
      console.error("Image insert failed:", error);
    }
  }, [saveImage]);

  const addImageFromFile = useCallback(() => {
    pickImageFile(insertImageFile);
  }, [insertImageFile]);

  const handlePaste = useCallback((_view: unknown, event: ClipboardEvent) => {
    const items = event.clipboardData?.items;
    if (items) {
      for (const item of items) {
        if (item.type.startsWith("image/")) {
          event.preventDefault();
          const file = item.getAsFile();
          if (file) void insertImageFile(file);
          return true;
        }
      }
    }
    return false;
  }, [insertImageFile]);

  const handleDrop = useCallback((_view: unknown, event: DragEvent) => {
    const files = event.dataTransfer?.files;
    if (files && files.length > 0) {
      for (const file of files) {
        if (file.type.startsWith("image/")) {
          event.preventDefault();
          void insertImageFile(file);
          return true;
        }
      }
    }
    return false;
  }, [insertImageFile]);

  useEffect(() => {
    if (!editor) return;
    const nextContent = note.content || "";
    let cancelled = false;
    isApplyingExternalContentRef.current = true;
    void (async () => {
      try {
        const hydrated = await hydrateContent(nextContent);
        if (cancelled) return;
        if (nextContent !== lastAppliedContentRef.current || editor.isEmpty) {
          editor.commands.setContent(hydrated, { emitUpdate: false });
          lastAppliedContentRef.current = nextContent;
          markEditorChangedRef.current();
        }
      } finally {
        if (!cancelled) isApplyingExternalContentRef.current = false;
      }
    })().catch((error) => console.error("Attachment hydration failed:", error));
    return () => {
      cancelled = true;
    };
  }, [editor, hydrateContent, note.content, note.id]);

  useEffect(() => {
    if (!editor || !migrateContent || !shouldMigrateContent?.(note) || migratedNotesRef.current.has(note.id)) return;
    migratedNotesRef.current.add(note.id);

    let cancelled = false;
    const migrate = async () => {
      const nextContent = await migrateContent(note.content);
      if (!cancelled && nextContent !== note.content) {
        const hydrated = await hydrateContent(nextContent);
        editor.commands.setContent(hydrated, { emitUpdate: false });
        lastAppliedContentRef.current = nextContent;
        onUpdate(note.id, nextContent);
      }
    };

    migrate().catch((error) => console.error("Image migration failed:", error));
    return () => {
      cancelled = true;
    };
  }, [editor, hydrateContent, migrateContent, note, onUpdate, shouldMigrateContent]);

  return {
    findReplace,
    setEditor,
    addImageFromFile,
    handleEditorUpdate,
    editorProps: {
      attributes: {
        class: "tiptap prose prose-sm max-w-none focus:outline-none px-8 py-6 min-h-full",
      },
      handlePaste,
      handleDrop,
    },
  };
}
