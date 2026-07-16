import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { runTests } from "@vscode/test-electron";

await import("../../packages/vscode/build.mjs");

const root = resolve(import.meta.dirname, "../..");
const markless = resolve(
  process.env.MARKLESS_ROOT ?? "/Users/jacksm5pro/dev/open-source/markless",
);
const marklessSource = join(
  markless,
  "packages/vitest-browser/browser/fixtures/arm-try-events.tsrx",
);
const marklessExtension = join(markless, "packages/vscode-plugin");
const extension = join(root, "packages/vscode");
const server = join(root, "target/release/oxc-tsrx-lsp");
const executable =
  process.env.VSCODE_EXECUTABLE_PATH ??
  "/Applications/Visual Studio Code.app/Contents/MacOS/Electron";

function externalFingerprint() {
  const status = execFileSync("git", ["status", "--porcelain=v1", "-z"], {
    cwd: markless,
  });
  const diff = execFileSync("git", ["diff", "--binary"], { cwd: markless });
  return createHash("sha256").update(status).update(diff).digest("hex");
}

const before = externalFingerprint();
const marklessHead = execFileSync("git", ["rev-parse", "HEAD"], {
  cwd: markless,
  encoding: "utf8",
}).trim();
const sourceSha256 = createHash("sha256")
  .update(await readFile(marklessSource))
  .digest("hex");
const workspace = await mkdtemp(join(tmpdir(), "oxc-tsrx-vscode-"));
await mkdir(join(workspace, ".vscode"), { recursive: true });
await mkdir(join(workspace, "config"), { recursive: true });
const sourcePath = join(workspace, "App.tsrx");
await cp(marklessSource, sourcePath);
let source = await readFile(sourcePath, "utf8");
source = source
  .replace(
    "export function App() @{",
    "export function App() @{\nvar editorProbe=0;\nvoid editorProbe;\ndebugger;",
  )
  .replace("let saved = state('none');", "let saved=state('none');");
await writeFile(sourcePath, source);
await writeFile(
  join(workspace, ".oxlintrc.json"),
  `${JSON.stringify({ rules: { "no-debugger": "off", "no-var": "off" } }, null, 2)}\n`,
);
await writeFile(
  join(workspace, "config/strict.json"),
  `${JSON.stringify({ rules: { "no-debugger": "error", "no-var": "error" } }, null, 2)}\n`,
);
await writeFile(
  join(workspace, "config/no-var-only.json"),
  `${JSON.stringify({ rules: { "no-debugger": "off", "no-var": "error" } }, null, 2)}\n`,
);
await writeFile(
  join(workspace, ".oxfmtrc.json"),
  `${JSON.stringify({ semi: true, singleQuote: true }, null, 2)}\n`,
);
await writeFile(
  join(workspace, ".vscode/settings.json"),
  `${JSON.stringify(
    {
      "oxcTsrx.lint.configPath": "config/strict.json",
      "[markless-tsrx]": {
        "editor.defaultFormatter": "thejackshelton.oxc-tsrx-vscode",
        "editor.formatOnSave": true,
      },
    },
    null,
    2,
  )}\n`,
);

let passed = false;
try {
  await runTests({
    vscodeExecutablePath: executable,
    reuseMachineInstall: false,
    extensionDevelopmentPath: [extension, marklessExtension],
    extensionTestsPath: join(root, "tests/editor/vscode-suite.cjs"),
    extensionTestsEnv: {
      ...process.env,
      OXC_TSRX_LSP_BIN: server,
      OXC_TSRX_EDITOR_FILE: sourcePath,
    },
    launchArgs: [
      workspace,
      `--extensions-dir=${join(workspace, ".vscode-extensions")}`,
      `--user-data-dir=${join(workspace, ".vscode-user")}`,
      "--disable-extensions",
      "--disable-workspace-trust",
      "--skip-welcome",
      "--skip-release-notes",
    ],
  });
  passed = true;
} finally {
  const after = externalFingerprint();
  assert.equal(after, before, "the read-only Markless worktree changed");
  if (passed) {
    const vscodeManifest = JSON.parse(
      await readFile(
        "/Applications/Visual Studio Code.app/Contents/Resources/app/package.json",
        "utf8",
      ),
    );
    await writeFile(
      join(root, "tests/editor/markless-vscode-walkthrough.json"),
      `${JSON.stringify(
        {
          schemaVersion: 1,
          recordedAt: new Date().toISOString(),
          vscodeVersion: vscodeManifest.version,
          markless: {
            head: marklessHead,
            source: "packages/vitest-browser/browser/fixtures/arm-try-events.tsrx",
            sourceSha256,
            beforeFingerprint: before,
            afterFingerprint: after,
            externalWrites: false,
          },
          extension: {
            id: "thejackshelton.oxc-tsrx-vscode",
            frameworkLanguageId: "markless-tsrx",
            nativeServer: "target/release/oxc-tsrx-lsp",
            bundledClient: true,
          },
          assertions: {
            automaticActivation: true,
            liveAuthoredDiagnostics: true,
            configurationLifecycle: true,
            realFormatOnSave: true,
            safeCodeAction: true,
            diagnosticsUpdatedAfterAction: true,
          },
        },
        null,
        2,
      )}\n`,
    );
  }
}
