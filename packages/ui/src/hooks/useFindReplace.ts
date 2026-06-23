import { useEffect, useMemo, useState } from "react";

export interface TextMatch {
  from: number;
  to: number;
  text: string;
}

export interface FindReplaceControls {
  visible: boolean;
  setVisible: React.Dispatch<React.SetStateAction<boolean>>;
  findQuery: string;
  setFindQuery: (query: string) => void;
  replaceQuery: string;
  setReplaceQuery: (query: string) => void;
  useRegex: boolean;
  setUseRegex: React.Dispatch<React.SetStateAction<boolean>>;
  currentMatchIndex: number;
  findState: { matches: TextMatch[]; error: string | null };
  goToMatch: (direction: 1 | -1) => void;
  replaceCurrent: () => void;
  replaceAll: () => void;
  markEditorChanged: () => void;
}

export function useFindReplace(editor: any, noteId: string): FindReplaceControls {
  const [findQuery, setFindQuery] = useState("");
  const [replaceQuery, setReplaceQuery] = useState("");
  const [useRegex, setUseRegex] = useState(false);
  const [currentMatchIndex, setCurrentMatchIndex] = useState(0);
  const [editorRevision, setEditorRevision] = useState(0);
  const [visible, setVisible] = useState(false);

  const findState = useMemo(
    () => (editor ? collectTextMatches(editor, findQuery, useRegex, editorRevision) : { matches: [], error: null }),
    [editor, editorRevision, findQuery, useRegex],
  );

  useEffect(() => {
    setCurrentMatchIndex(0);
  }, [findQuery, useRegex, noteId]);

  useEffect(() => {
    if (!editor || findState.matches.length === 0) return;
    const match = findState.matches[Math.min(currentMatchIndex, findState.matches.length - 1)];
    if (!match) return;
    editor.chain().focus().setTextSelection({ from: match.from, to: match.to }).run();
  }, [currentMatchIndex, editor, findState.matches]);

  const markEditorChanged = () => {
    setEditorRevision((revision) => revision + 1);
  };

  const goToMatch = (direction: 1 | -1) => {
    if (findState.matches.length === 0) return;
    setCurrentMatchIndex((index) => (index + direction + findState.matches.length) % findState.matches.length);
  };

  const replaceCurrent = () => {
    if (!editor) return;
    const match = findState.matches[currentMatchIndex];
    if (!match || findState.error) return;
    editor
      .chain()
      .focus()
      .insertContentAt(
        { from: match.from, to: match.to },
        buildReplacement(match.text, findQuery, replaceQuery, useRegex),
      )
      .run();
    markEditorChanged();
  };

  const replaceAll = () => {
    if (!editor || findState.matches.length === 0 || findState.error) return;
    for (const match of [...findState.matches].reverse()) {
      editor
        .chain()
        .insertContentAt(
          { from: match.from, to: match.to },
          buildReplacement(match.text, findQuery, replaceQuery, useRegex),
        )
        .run();
    }
    editor.commands.focus();
    setCurrentMatchIndex(0);
    markEditorChanged();
  };

  return {
    visible,
    setVisible,
    findQuery,
    setFindQuery,
    replaceQuery,
    setReplaceQuery,
    useRegex,
    setUseRegex,
    currentMatchIndex,
    findState,
    goToMatch,
    replaceCurrent,
    replaceAll,
    markEditorChanged,
  };
}

function collectTextMatches(
  editor: any,
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
