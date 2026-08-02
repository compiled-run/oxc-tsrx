export interface OxcTsrxCommandInvocation {
  readonly file: string;
  readonly args: string[];
  readonly windowsVerbatimArguments: boolean;
}

export declare function escapeCommandArgument(argument: string): string;

export declare function resolveCommandInvocation(
  file: string,
  args?: readonly string[],
  platform?: NodeJS.Platform,
): OxcTsrxCommandInvocation;

export declare function spawnCommand(
  file: string,
  args?: readonly string[],
  options?: import("node:child_process").SpawnOptions,
  spawnProcess?: typeof import("node:child_process").spawn,
): import("node:child_process").ChildProcess;
