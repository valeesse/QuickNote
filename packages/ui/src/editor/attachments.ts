export function createAttachmentImageExtension(ImageExtension: any) {
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
  });
}
