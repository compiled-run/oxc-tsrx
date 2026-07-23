const noTsrxIf = {
  meta: {
    type: "suggestion",
    docs: {
      description: "Demo a JavaScript rule visiting authored TSRX control syntax",
    },
    messages: {
      avoid: "Demo rule: prefer a declarative component over this TSRX @if block.",
    },
    schema: [],
  },
  create(context) {
    return {
      JSXIfExpression(node) {
        context.report({ node, messageId: "avoid" });
      },
    };
  },
};

const requireKeyedFor = {
  meta: {
    type: "problem",
    docs: {
      description: "Require a key expression on TSRX @for blocks",
    },
    messages: {
      missing: "TSRX @for blocks should declare `key <expression>`.",
    },
    schema: [],
  },
  create(context) {
    return {
      JSXForExpression(node) {
        if (node.key == null) context.report({ node, messageId: "missing" });
      },
    };
  },
};

export default {
  meta: {
    name: "eslint-plugin-tsrx-demo",
    version: "0.1.0",
  },
  rules: {
    "no-tsrx-if": noTsrxIf,
    "require-keyed-for": requireKeyedFor,
  },
};
