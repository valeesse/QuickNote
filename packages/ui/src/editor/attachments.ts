export function createAttachmentImageExtension(
  ImageExtension: any,
  resolveImageSrc?: (attachmentId: string) => Promise<string>,
) {
  return ImageExtension.extend({
    addAttributes() {
      return {
        ...this.parent?.(),
        attachmentId: {
          default: null,
          parseHTML: (element: HTMLElement) =>
            element.getAttribute("data-attachment-id") ||
            element.getAttribute("src")?.match(/^attachment:\/\/(.+)$/)?.[1] ||
            null,
          renderHTML: (attributes: { attachmentId?: string | null }) =>
            attributes.attachmentId
              ? { "data-attachment-id": attributes.attachmentId }
              : {},
        },
      };
    },
    addNodeView() {
      if (!resolveImageSrc) return null;
      return ({ node }: { node: { type: unknown; attrs: Record<string, unknown> } }) => {
        const image = document.createElement("img");
        let currentId = "";
        let disposed = false;
        const render = (attrs: Record<string, unknown>) => {
          const id = typeof attrs.attachmentId === "string" ? attrs.attachmentId : "";
          image.alt = typeof attrs.alt === "string" ? attrs.alt : "";
          image.title = typeof attrs.title === "string" ? attrs.title : "";
          image.dataset.attachmentId = id;
          if (!id) {
            image.src = typeof attrs.src === "string" ? attrs.src : "";
            return;
          }
          if (id === currentId) return;
          currentId = id;
          image.removeAttribute("src");
          void resolveImageSrc(id).then((src) => {
            if (!disposed && currentId === id) image.src = src;
          }).catch(() => {
            if (!disposed && currentId === id) image.alt ||= "附件无法加载";
          });
        };
        render(node.attrs);
        return {
          dom: image,
          update: (next: { type: unknown; attrs: Record<string, unknown> }) => {
            if (next.type !== node.type) return false;
            render(next.attrs);
            return true;
          },
          destroy: () => { disposed = true; },
        };
      };
    },
  });
}
