import type { SaveStatus } from "@contracts";

export function formatSaveStatus(status: SaveStatus, errorMessage: string | null): string {
  if (status === "saving") return "保存中";
  if (status === "retrying") return "重试保存中";
  if (status === "saved") return "已保存";
  if (status === "error") return errorMessage ? `保存失败：${errorMessage}` : "保存失败";
  return "未修改";
}

export function sanitizeFilename(value: string): string {
  return value.replace(/[\\/:*?"<>|]/g, "_").slice(0, 80) || "QuickNote";
}

export function formatRelativeTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMs / 3600000);
  const diffDays = Math.floor(diffMs / 86400000);

  if (diffMins < 1) return "刚刚";
  if (diffMins < 60) return `${diffMins}分钟前`;
  if (diffHours < 24) return `${diffHours}小时前`;
  if (diffDays < 7) return `${diffDays}天前`;
  return date.toLocaleDateString("zh-CN", { month: "short", day: "numeric" });
}
