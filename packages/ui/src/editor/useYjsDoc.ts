import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { IndexeddbPersistence } from "y-indexeddb";
import { Awareness, applyAwarenessUpdate, encodeAwarenessUpdate } from "y-protocols/awareness";
import * as Y from "yjs";
import { base64ToBytes, bytesToBase64, encodeStateVector, encodeUpdateFrame, isEmptyUpdate, type ServerControl } from "./yjsProtocol";

const remoteOrigin = Symbol("quicknote-remote-yjs-update");
type Connection = "local" | "connecting" | "connected" | "offline";
type Delivery = "idle" | "saving" | "saved" | "retrying" | "error";

export interface YjsDocState {
  doc: Y.Doc;
  awareness: Awareness;
  isCollaborative: boolean;
  isReady: boolean;
  shouldBootstrap: boolean;
  connection: Connection;
  delivery: Delivery;
  sendProjection: (html: string) => void;
}

export function useYjsDoc({
  noteId, state, stateVersion, websocketUrl, collaborative = false,
}: {
  noteId: string;
  state?: number[] | Uint8Array | null;
  stateVersion?: number | null;
  websocketUrl?: string | null;
  collaborative?: boolean;
}): YjsDocState {
  const doc = useMemo(() => new Y.Doc({ guid: noteId }), [noteId]);
  const awareness = useMemo(() => {
    const next = new Awareness(doc);
    next.setLocalStateField("user", cursorUser(doc.clientID));
    return next;
  }, [doc]);
  const socketRef = useRef<WebSocket | null>(null);
  const readyRef = useRef(false);
  const connectionRef = useRef<Connection>(websocketUrl ? "connecting" : "local");
  const pendingUpdatesRef = useRef(new Set<string>());
  const projectionRef = useRef({ html: "", dirty: false, pendingId: "", revision: 0, pendingRevision: 0 });
  const projectionTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [isReady, setReady] = useState(false);
  const [shouldBootstrap, setShouldBootstrap] = useState(!state?.length);
  const [connection, setConnection] = useState<Connection>(websocketUrl ? "connecting" : "local");
  const [delivery, setDelivery] = useState<Delivery>("idle");

  const updateDelivery = useCallback(() => {
    if (connectionRef.current === "offline") return setDelivery("retrying");
    if (connectionRef.current === "connecting") return setDelivery("saving");
    const projection = projectionRef.current;
    setDelivery(
      pendingUpdatesRef.current.size || projection.dirty || projection.pendingId ? "saving" : "saved",
    );
  }, []);

  const updateConnection = useCallback((next: Connection) => {
    connectionRef.current = next;
    setConnection(next);
  }, []);

  useEffect(() => {
    readyRef.current = isReady;
    updateDelivery();
  }, [isReady, updateDelivery]);

  useEffect(() => {
    if (websocketUrl) return;
    if (state?.length) Y.applyUpdate(doc, Uint8Array.from(state), remoteOrigin);
    readyRef.current = true;
    setReady(true);
    setDelivery("saved");
  }, [doc, stateVersion, websocketUrl]);

  useEffect(() => {
    if (!websocketUrl) return;
    let disposed = false;
    let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
    let attempt = 0;
    let receivedControl = false;
    const persistence = new IndexeddbPersistence(`quicknote-yjs-${noteId}`, doc);
    awareness.setLocalStateField("user", cursorUser(doc.clientID));

    const sendUpdate = (socket: WebSocket, update: Uint8Array) => {
      if (isEmptyUpdate(update)) return;
      const updateId = crypto.randomUUID();
      pendingUpdatesRef.current.add(updateId);
      socket.send(encodeUpdateFrame(updateId, update));
      setDelivery("saving");
    };
    const sendProjectionNow = (socket: WebSocket) => {
      const projection = projectionRef.current;
      if (!readyRef.current || !projection.dirty) return;
      const projectionId = crypto.randomUUID();
      projection.pendingId = projectionId;
      projection.pendingRevision = projection.revision;
      socket.send(JSON.stringify({
        type: "projection", projection_id: projectionId, html: projection.html,
        state_vector: encodeStateVector(doc),
      }));
      setDelivery("saving");
    };
    const scheduleProjection = (delay = 100) => {
      if (projectionTimerRef.current) clearTimeout(projectionTimerRef.current);
      projectionTimerRef.current = setTimeout(() => {
        projectionTimerRef.current = null;
        const socket = socketRef.current;
        if (socket?.readyState === WebSocket.OPEN) sendProjectionNow(socket);
      }, delay);
    };
    const handleControl = (control: ServerControl) => {
      if (control.type === "sync") {
        receivedControl = true;
        setShouldBootstrap(control.bootstrap);
      } else if (control.type === "ack") {
        pendingUpdatesRef.current.delete(control.update_id);
        updateDelivery();
      } else if (control.type === "projection_ack") {
        if (projectionRef.current.pendingId === control.projection_id) {
          projectionRef.current.pendingId = "";
          projectionRef.current.dirty =
            projectionRef.current.pendingRevision !== projectionRef.current.revision;
          if (projectionRef.current.dirty) scheduleProjection();
        }
        updateDelivery();
      } else if (control.type === "projection_rejected") {
        if (projectionRef.current.pendingId === control.projection_id) {
          projectionRef.current.pendingId = "";
          projectionRef.current.dirty = true;
          scheduleProjection();
        }
      } else if (control.type === "awareness") {
        applyAwarenessUpdate(awareness, base64ToBytes(control.update), remoteOrigin);
      } else if (control.type === "error") {
        setDelivery("error");
      }
    };
    const connect = () => {
      if (disposed) return;
      receivedControl = false;
      let receivedState = false;
      updateConnection(attempt === 0 ? "connecting" : "offline");
      const socket = new WebSocket(websocketUrl);
      socket.binaryType = "arraybuffer";
      socketRef.current = socket;
      socket.addEventListener("open", () => {
        attempt = 0;
        updateConnection("connected");
        updateDelivery();
        socket.send(JSON.stringify({
          type: "awareness",
          update: bytesToBase64(encodeAwarenessUpdate(awareness, [doc.clientID])),
        }));
      });
      socket.addEventListener("message", (event) => {
        if (typeof event.data === "string") {
          try { handleControl(JSON.parse(event.data) as ServerControl); } catch { /* future control */ }
          return;
        }
        const apply = (buffer: ArrayBuffer) => {
          const update = new Uint8Array(buffer);
          const initialServerState = !receivedState && receivedControl;
          const serverVector = initialServerState ? Y.encodeStateVectorFromUpdate(update) : null;
          Y.applyUpdate(doc, update, remoteOrigin);
          if (initialServerState && serverVector) {
            receivedState = true;
            pendingUpdatesRef.current.clear();
            if (!readyRef.current) {
              readyRef.current = true;
            setReady(true);
            }
            sendUpdate(socket, Y.encodeStateAsUpdate(doc, serverVector));
            if (projectionRef.current.dirty) scheduleProjection();
            updateDelivery();
          }
        };
        if (event.data instanceof ArrayBuffer) apply(event.data);
        else if (event.data instanceof Blob) void event.data.arrayBuffer().then(apply);
      });
      socket.addEventListener("close", () => {
        if (socketRef.current === socket) socketRef.current = null;
        if (disposed) return;
        updateConnection("offline");
        setDelivery("retrying");
        reconnectTimer = setTimeout(connect, Math.min(1_000 * 2 ** attempt++, 15_000));
      });
    };
    const handleUpdate = (update: Uint8Array, origin: unknown) => {
      if (origin === remoteOrigin || !readyRef.current) return;
      const socket = socketRef.current;
      if (socket?.readyState === WebSocket.OPEN) sendUpdate(socket, update);
      else setDelivery("retrying");
    };
    const handleAwareness = (
      { added, updated, removed }: { added: number[]; updated: number[]; removed: number[] },
      origin: unknown,
    ) => {
      if (origin === remoteOrigin) return;
      const socket = socketRef.current;
      if (socket?.readyState !== WebSocket.OPEN) return;
      const clients = [...added, ...updated, ...removed];
      socket.send(JSON.stringify({
        type: "awareness", update: bytesToBase64(encodeAwarenessUpdate(awareness, clients)),
      }));
    };
    doc.on("update", handleUpdate);
    awareness.on("update", handleAwareness);
    void persistence.whenSynced.catch(() => undefined).then(connect);
    return () => {
      disposed = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      if (projectionTimerRef.current) clearTimeout(projectionTimerRef.current);
      doc.off("update", handleUpdate);
      awareness.setLocalState(null);
      awareness.off("update", handleAwareness);
      socketRef.current?.close();
      socketRef.current = null;
      persistence.destroy();
    };
  }, [awareness, doc, noteId, updateConnection, updateDelivery, websocketUrl]);

  useEffect(() => () => doc.destroy(), [doc]);

  const sendProjection = useCallback((html: string) => {
    const projection = projectionRef.current;
    projection.html = html;
    projection.dirty = true;
    projection.revision += 1;
    setDelivery(socketRef.current?.readyState === WebSocket.OPEN ? "saving" : "retrying");
    if (projectionTimerRef.current) clearTimeout(projectionTimerRef.current);
    projectionTimerRef.current = setTimeout(() => {
      projectionTimerRef.current = null;
      const socket = socketRef.current;
      if (!readyRef.current || socket?.readyState !== WebSocket.OPEN) return;
      const projectionId = crypto.randomUUID();
      projection.pendingId = projectionId;
      projection.pendingRevision = projection.revision;
      socket.send(JSON.stringify({
        type: "projection", projection_id: projectionId, html: projection.html,
        state_vector: encodeStateVector(doc),
      }));
    }, 700);
  }, [doc]);

  return {
    doc, awareness, isCollaborative: Boolean(collaborative || websocketUrl || state?.length),
    isReady, shouldBootstrap, connection, delivery, sendProjection,
  };
}

function cursorColor(clientId: number): string {
  const colors = ["#2563eb", "#7c3aed", "#db2777", "#059669", "#d97706", "#0891b2"];
  return colors[clientId % colors.length] ?? colors[0]!;
}

function cursorUser(clientId: number): { name: string; color: string } {
  return { name: `设备 ${String(clientId).slice(-4)}`, color: cursorColor(clientId) };
}
