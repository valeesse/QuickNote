import { ArrowUp } from "lucide-react";
import { useEffect, useState, type RefObject } from "react";

export function ScrollToTopButton<T extends HTMLElement>({
  targetRef,
  className = "",
  threshold = 320,
}: {
  targetRef: RefObject<T | null>;
  className?: string;
  threshold?: number;
}) {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const target = targetRef.current;
    if (!target) return;
    const updateVisibility = () => setVisible(target.scrollTop > threshold);
    updateVisibility();
    target.addEventListener("scroll", updateVisibility, { passive: true });
    return () => target.removeEventListener("scroll", updateVisibility);
  }, [targetRef, threshold]);

  if (!visible) return null;

  return (
    <button
      type="button"
      className={`focus-ring scroll-to-top ${className}`.trim()}
      onClick={() => {
        const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
        targetRef.current?.scrollTo({ top: 0, behavior: reduceMotion ? "auto" : "smooth" });
      }}
      title="返回顶部"
      aria-label="返回顶部"
    >
      <ArrowUp aria-hidden="true" />
      <span>顶部</span>
    </button>
  );
}
