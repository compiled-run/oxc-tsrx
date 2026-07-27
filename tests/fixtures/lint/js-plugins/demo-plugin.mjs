// An ordinary Oxlint JavaScript plugin. Nothing here knows what TSRX is: it is
// the same module shape that already runs on .js, .ts, .jsx, and .tsx files.
//
// The rule reports any identifier literally named `banned`, which makes the
// reported span exactly six bytes long and easy to check by hand against the
// authored source.
const noBannedIdentifier = {
  meta: {
    type: "problem",
    docs: { description: "Ban the identifier `banned`" },
    messages: { notAllowed: "`banned` is not an allowed identifier." },
    schema: [],
  },
  create(context) {
    return {
      Identifier(node) {
        if (node.name !== "banned") return;
        context.report({ node, messageId: "notAllowed" });
      },
    };
  },
};

// Reports the file Oxlint told the rule it was linting, so the test can pin what
// `context.filename` looks like on the .tsrx path rather than guess at it.
const reportFilename = {
  meta: {
    type: "suggestion",
    docs: { description: "Report context.filename, for the position tests" },
    schema: [],
  },
  create(context) {
    return {
      DebuggerStatement(node) {
        context.report({ node, message: `context.filename=${context.filename}` });
      },
    };
  },
};

export default {
  meta: { name: "tsrx-js-demo", version: "0.1.0" },
  rules: {
    "no-banned-identifier": noBannedIdentifier,
    "report-filename": reportFilename,
  },
};
