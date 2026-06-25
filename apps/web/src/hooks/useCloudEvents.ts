import { useEffect, useRef } from "react";
import { getBaseUrl } from "@/api/client";

export function useCloudEvents(onChange: () => void): void {
  const callbackRef = useRef(onChange);
  useEffect(() => { callbackRef.current = onChange; }, [onChange]);

  useEffect(() => {
    const controller = new AbortController();
    let retry: ReturnType<typeof setTimeout> | undefined;

    const connect = async () => {
      try {
        const response = await fetch(`${getBaseUrl()}/api/events`, {
          credentials: "include",
          signal: controller.signal,
        });
        if (!response.ok || !response.body) throw new Error(`Event stream failed (${response.status})`);
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";
        while (!controller.signal.aborted) {
          const { done, value } = await reader.read();
          if (done) break;
          buffer += decoder.decode(value, { stream: true });
          const events = buffer.split("\n\n");
          buffer = events.pop() ?? "";
          if (events.some((event) => event.split("\n").some((line) => line.startsWith("data:")))) {
            callbackRef.current();
          }
        }
      } catch (error) {
        if (!controller.signal.aborted) {
          console.warn("Cloud event stream disconnected", error);
          retry = setTimeout(() => void connect(), 3000);
        }
      }
    };
    void connect();
    return () => { controller.abort(); if (retry) clearTimeout(retry); };
  }, []);
}
