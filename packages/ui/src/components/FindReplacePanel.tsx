import { Check, Regex, Replace, Search, X } from "lucide-react";
import type { FindReplaceControls } from "../hooks/useFindReplace";

export function FindReplacePanel({ controls }: { controls: FindReplaceControls }) {
  const {
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
    setVisible,
  } = controls;

  return (
    <div className="absolute right-6 top-12 z-30 w-[min(520px,calc(100vw-2rem))] rounded-lg border border-gray-200 bg-white p-3 text-xs shadow-xl">
      <div className="mb-2 flex items-center justify-between">
        <span className="font-medium text-gray-700">查找替换</span>
        <button
          type="button"
          onClick={() => setVisible(false)}
          className="focus-ring rounded p-1 text-gray-400 hover:bg-gray-100 hover:text-gray-600"
          title="关闭"
          aria-label="关闭查找替换"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
      <div className="grid gap-2">
        <label className="relative">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-gray-400" />
          <input
            value={findQuery}
            onChange={(event) => setFindQuery(event.target.value)}
            placeholder="查找"
            autoFocus
            className={`h-8 w-full rounded-lg border bg-gray-50 pl-8 pr-3 outline-none transition focus:bg-white focus:ring-2 ${
              findState.error ? "border-red-200 focus:ring-red-100" : "border-gray-200 focus:border-blue-300 focus:ring-blue-100"
            }`}
          />
        </label>
        <label className="relative">
          <Replace className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-gray-400" />
          <input
            value={replaceQuery}
            onChange={(event) => setReplaceQuery(event.target.value)}
            placeholder="替换为"
            className="h-8 w-full rounded-lg border border-gray-200 bg-gray-50 pl-8 pr-3 outline-none transition focus:border-blue-300 focus:bg-white focus:ring-2 focus:ring-blue-100"
          />
        </label>
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => setUseRegex((value) => !value)}
            aria-pressed={useRegex}
            className={`focus-ring flex h-8 items-center gap-1 rounded-lg border px-2 ${useRegex ? "border-blue-200 bg-blue-50 text-blue-700" : "border-gray-200 text-gray-500 hover:bg-gray-50"}`}
            title="使用正则"
          >
            <Regex className="h-3.5 w-3.5" />
            正则
          </button>
          <span className={`min-w-[64px] text-center ${findState.error ? "text-red-500" : "text-gray-400"}`}>
            {findState.error ? "正则错误" : findQuery ? `${findState.matches.length ? currentMatchIndex + 1 : 0}/${findState.matches.length}` : "0/0"}
          </span>
          <button type="button" onClick={() => goToMatch(-1)} disabled={findState.matches.length === 0} className="focus-ring h-8 rounded-lg border border-gray-200 px-2 text-gray-600 hover:bg-gray-50 disabled:opacity-40">上一个</button>
          <button type="button" onClick={() => goToMatch(1)} disabled={findState.matches.length === 0} className="focus-ring h-8 rounded-lg border border-gray-200 px-2 text-gray-600 hover:bg-gray-50 disabled:opacity-40">下一个</button>
          <button type="button" onClick={replaceCurrent} disabled={findState.matches.length === 0 || Boolean(findState.error)} className="focus-ring flex h-8 items-center gap-1 rounded-lg border border-gray-200 px-2 text-gray-600 hover:bg-gray-50 disabled:opacity-40">
            <Check className="h-3.5 w-3.5" />
            替换
          </button>
          <button type="button" onClick={replaceAll} disabled={findState.matches.length === 0 || Boolean(findState.error)} className="focus-ring h-8 rounded-lg bg-gray-900 px-3 font-medium text-white hover:bg-gray-800 disabled:opacity-40">全部</button>
        </div>
      </div>
    </div>
  );
}
