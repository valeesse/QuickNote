import * as Y from "yjs";

const UPDATE_MAGIC = new Uint8Array([0x51, 0x4e, 0x55, 0x50]);

export type ServerControl =
  | { type: "sync"; bootstrap: boolean; state_version: number }
  | { type: "ack"; update_id: string; state_version: number }
  | { type: "projection_ack"; projection_id: string; state_version: number }
  | { type: "projection_rejected"; projection_id: string; state_version: number }
  | { type: "awareness"; update: string }
  | { type: "error" };

export function encodeUpdateFrame(updateId: string, update: Uint8Array): ArrayBuffer {
  const frame = new Uint8Array(20 + update.byteLength);
  frame.set(UPDATE_MAGIC, 0);
  frame.set(uuidBytes(updateId), 4);
  frame.set(update, 20);
  return frame.buffer;
}

export function encodeStateVector(doc: Y.Doc): string {
  return bytesToBase64(Y.encodeStateVector(doc));
}

export function bytesToBase64(vector: Uint8Array): string {
  let binary = "";
  for (const byte of vector) binary += String.fromCharCode(byte);
  return btoa(binary);
}

export function base64ToBytes(value: string): Uint8Array {
  return Uint8Array.from(atob(value), (character) => character.charCodeAt(0));
}

export function isEmptyUpdate(update: Uint8Array): boolean {
  return update.byteLength === 2 && update[0] === 0 && update[1] === 0;
}

function uuidBytes(value: string): Uint8Array {
  const hex = value.replace(/-/g, "");
  if (!/^[0-9a-f]{32}$/i.test(hex)) throw new Error("Invalid update identifier");
  return Uint8Array.from({ length: 16 }, (_, index) =>
    Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16),
  );
}
