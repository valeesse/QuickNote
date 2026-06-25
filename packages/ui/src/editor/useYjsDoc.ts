import { useEffect, useMemo } from "react";
import * as Y from "yjs";

export interface YjsDocState {
  doc: Y.Doc;
  isCollaborative: boolean;
}

export function useYjsDoc({
  noteId,
  state,
  stateVersion,
}: {
  noteId: string;
  state?: number[] | Uint8Array | null;
  stateVersion?: number | null;
}): YjsDocState {
  const doc = useMemo(() => {
    const nextDoc = new Y.Doc({ guid: noteId });
    if (state?.length) {
      Y.applyUpdate(nextDoc, Uint8Array.from(state));
    }
    return nextDoc;
  }, [noteId, stateVersion]);

  useEffect(() => () => doc.destroy(), [doc]);

  return {
    doc,
    isCollaborative: Boolean(state?.length),
  };
}
