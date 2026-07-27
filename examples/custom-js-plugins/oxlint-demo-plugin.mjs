// An Oxlint JavaScript plugin. The default export is `{ meta, rules }`, and
// each rule's `create(context)` returns a visitor keyed by node type. Oxlint
// runs this on ordinary .js/.ts/.jsx/.tsx files.

function hasKeyProp(element) {
  return element.openingElement.attributes.some(
    (attribute) => attribute.type === "JSXAttribute" && attribute.name.name === "key",
  );
}

const requireKeyedMap = {
  meta: {
    type: "problem",
    docs: { description: "Require a key prop on JSX returned straight from .map()" },
    messages: { missing: "JSX returned from .map() should declare a `key` prop." },
    schema: [],
  },
  create(context) {
    return {
      CallExpression(node) {
        if (node.callee.type !== "MemberExpression") return;
        if (node.callee.property.name !== "map") return;
        const returned = node.arguments[0]?.body;
        if (returned?.type !== "JSXElement" || hasKeyProp(returned)) return;
        context.report({ node: returned, messageId: "missing" });
      },
    };
  },
};

export default {
  meta: { name: "tsrx-demo", version: "0.1.0" },
  rules: { "require-keyed-map": requireKeyedMap },
};
