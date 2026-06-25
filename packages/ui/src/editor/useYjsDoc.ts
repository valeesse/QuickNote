import { useEffect, useMemo } from "react";
import * as Y from "yjs";

const remoteOrigin = Symbol("quicknote-remote-yjs-update");

export interface YjsDocState {
  doc: Y.Doc;
  isCollaborative: boolean;
}

export function useYjsDoc({
  noteId,
  state,
  stateVersion,
  websocketUrl,
}: {
  noteId: string;
  state?: number[] | Uint8Array | null;
  stateVersion?: number | null;
  websocketUrl?: string | null;
}): YjsDocState {
  const doc = useMemo(() => {
    const nextDoc = new Y.Doc({ guid: noteId });
    if (state?.length) {
      Y.applyUpdate(nextDoc, Uint8Array.from(state));
    }
    return nextDoc;
  }, [noteId, stateVersion]);

  useEffect(() => () => doc.destroy(), [doc]);

  useEffect(() => {
    if (!state?.length || !websocketUrl) return;

    const socket = new WebSocket(websocketUrl);
    socket.binaryType = "arraybuffer";

    const handleUpdate = (update: Uint8Array, origin: unknown) => {
      if (origin === remoteOrigin || socket.readyState !== WebSocket.OPEN) return;
      const payload = new ArrayBuffer(update.byteLength);
      new Uint8Array(payload).set(update);
      socket.send(payload);
    };

    const handleMessage = (event: MessageEvent) => {
      if (event.data instanceof ArrayBuffer) {
        Y.applyUpdate(doc, new Uint8Array(event.data), remoteOrigin);
      } else if (event.data instanceof Blob) {
        void event.data
          .arrayBuffer()
          .then((buffer) => Y.applyUpdate(doc, new Uint8Array(buffer), remoteOrigin));
      }
    };

    doc.on("update", handleUpdate);
    socket.addEventListener("message", handleMessage);

    return () => {
      doc.off("update", handleUpdate);
      socket.removeEventListener("message", handleMessage);
      socket.close();
    };
  }, [doc, state?.length, websocketUrl]);

  return {
    doc,
    isCollaborative: Boolean(state?.length),
  };
}
