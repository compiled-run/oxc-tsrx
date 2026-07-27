// A house rule: an ordinary Oxlint JavaScript plugin. The default export is
// `{ meta, rules }`, and each rule's `create(context)` returns a visitor keyed
// by AST node type. Nothing here is TSRX-specific.

const noInlineStyleObject = {
  meta: {
    type: "suggestion",
    docs: { description: "Prefer a class over an inline style object" },
    messages: { inline: "Inline `style={{ ... }}` object. Use a class instead." },
    schema: [],
  },
  create(context) {
    return {
      JSXAttribute(node) {
        if (node.name?.name !== "style") return;
        if (node.value?.type !== "JSXExpressionContainer") return;
        if (node.value.expression?.type !== "ObjectExpression") return;
        context.report({ node, messageId: "inline" });
      },
    };
  },
};

export default {
  meta: { name: "house-rules", version: "1.0.0" },
  rules: { "no-inline-style-object": noInlineStyleObject },
};
