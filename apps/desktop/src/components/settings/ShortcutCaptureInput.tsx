import React, { useEffect, useState } from "react";

// ── Shortcut key capture input ──

const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta"]);

function formatKey(event: KeyboardEvent): string {
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Win");

  const key = event.key;
  if (MODIFIER_KEYS.has(key)) return parts.join("+");

  let displayKey = key;
  if (key === " ") displayKey = "Space";
  else if (key.length === 1) displayKey = key.toUpperCase();

  parts.push(displayKey);
  return parts.join("+");
}

function hasValidModifier(shortcut: string): boolean {
  const parts = shortcut.split("+");
  const modifiers = parts.slice(0, -1);
  if (modifiers.length === 0) return false;
  const hasFunctionalModifier = modifiers.some(
    (m) => m === "Ctrl" || m === "Alt" || m === "Shift",
  );
  return hasFunctionalModifier;
}

export function ShortcutCaptureInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
}) {
  const [capturing, setCapturing] = useState(false);
  const [currentKeys, setCurrentKeys] = useState("");
  const [error, setError] = useState("");
  const inputRef = React.useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!capturing) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();

      const key = event.key;
      if (key === "Escape") {
        setCapturing(false);
        setCurrentKeys("");
        setError("");
        return;
      }
      if (key === "Backspace" || key === "Delete") {
        onChange("");
        setCapturing(false);
        setCurrentKeys("");
        setError("");
        return;
      }

      const combo = formatKey(event);
      if (!combo || MODIFIER_KEYS.has(event.key)) {
        setCurrentKeys(combo);
        return;
      }

      if (!hasValidModifier(combo)) {
        setError("需要 Ctrl/Alt/Shift 功能键参与");
        setCurrentKeys(combo);
        return;
      }

      setError("");
      onChange(combo);
      setCapturing(false);
      setCurrentKeys("");
    };

    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [capturing, onChange]);

  const displayValue = capturing
    ? currentKeys || "请按下快捷键组合…"
    : value || "未设置";

  return (
    <div>
      <button
        type="button"
        ref={inputRef as unknown as React.Ref<HTMLButtonElement>}
        onClick={() => {
          setCapturing((v) => !v);
          setError("");
          setCurrentKeys("");
        }}
        className={`w-full rounded-lg border px-3 py-2 font-mono text-sm text-left transition-colors ${
          capturing
            ? "border-emerald-400 bg-emerald-50 text-emerald-700 ring-2 ring-emerald-100"
            : value
              ? "border-gray-200 bg-gray-50 text-gray-800 hover:border-gray-300"
              : "border-gray-200 bg-white text-gray-400 hover:border-gray-300"
        }`}
        title={capturing ? "按 Esc 取消，按 Backspace 清除" : "点击后按下快捷键"}
      >
        <span className="flex items-center justify-between gap-2">
          <span className={capturing && !currentKeys ? "animate-pulse" : ""}>
            {displayValue}
          </span>
          {capturing && (
            <kbd className="rounded bg-white px-1.5 py-0.5 text-[10px] text-gray-400 border border-gray-200">
              Esc
            </kbd>
          )}
        </span>
      </button>
      {error && (
        <p className="mt-1 text-xs text-orange-600">{error}</p>
      )}
      {!value && !capturing && (
        <p className="mt-1 text-xs text-gray-400">{placeholder}</p>
      )}
    </div>
  );
}
