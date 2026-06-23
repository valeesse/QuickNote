import { Extension, InputRule } from "@tiptap/core";
import type { MarkType } from "@tiptap/pm/model";

/**
 * Inline Markdown mark rules — converts **bold**, *italic*, ~~strike~~, ==highlight==, `code`
 * as the user types.
 */
export const InlineMarkdownMarkRules = Extension.create({
  name: "inlineMarkdownMarkRules",

  addInputRules() {
    return INLINE_MARK_RULES.flatMap(({ markName, delimiter }) => {
      const type = this.editor.schema.marks[markName];
      return type ? [createDelimitedMarkRule(type, delimiter)] : [];
    });
  },
});

const INLINE_MARK_RULES = [
  { markName: "bold", delimiter: "**" },
  { markName: "bold", delimiter: "__" },
  { markName: "strike", delimiter: "~~" },
  { markName: "highlight", delimiter: "==" },
  { markName: "code", delimiter: "`" },
  { markName: "italic", delimiter: "*" },
  { markName: "italic", delimiter: "_" },
] as const;

function createDelimitedMarkRule(type: MarkType, delimiter: string) {
  return new InputRule({
    find: (text) => {
      const match = findDelimitedMark(text, delimiter);
      if (!match) return null;

      return {
        index: match.openStart,
        text: text.slice(match.openStart),
        data: {
          content: match.content,
          trailing: match.trailing,
        },
      };
    },
    handler: ({ state, range, match }) => {
      const content = match.data?.content as string | undefined;
      const trailing = (match.data?.trailing as string | undefined) || "";
      if (!content) return null;

      const { tr } = state;
      const trailingLength = trailing.length;
      const contentStart = range.from + delimiter.length;
      const contentEnd = contentStart + content.length;
      const closeStart = contentEnd;
      const closeEnd = range.to - trailingLength;

      tr.delete(closeStart, closeEnd);
      tr.delete(range.from, range.from + delimiter.length);
      tr.addMark(range.from, range.from + content.length, type.create());
      tr.removeStoredMark(type);
    },
  });
}

function findDelimitedMark(text: string, delimiter: string) {
  const trailing = text.endsWith(" ") ? " " : "";
  const closeEnd = text.length - trailing.length;
  const closeStart = closeEnd - delimiter.length;

  if (closeStart <= delimiter.length || text.slice(closeStart, closeEnd) !== delimiter) {
    return null;
  }

  const openStart = text.lastIndexOf(delimiter, closeStart - 1);
  if (openStart < 0) return null;

  if (delimiter.length === 1) {
    if (text[openStart - 1] === delimiter || text[closeEnd] === delimiter) {
      return null;
    }
  }

  const contentStart = openStart + delimiter.length;
  const content = text.slice(contentStart, closeStart);
  if (!content || content.trim() !== content) return null;

  return {
    openStart,
    content,
    trailing,
  };
}
