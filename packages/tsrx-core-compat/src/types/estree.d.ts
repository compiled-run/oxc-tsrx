export * from "@oxc-project/types";

export interface Position {
  line: number;
  column: number;
}

export interface SourceLocation {
  start: Position;
  end: Position;
}

export interface CommentWithLocation {
  type: "Line" | "Block";
  value: string;
  start: number;
  end: number;
  loc: SourceLocation;
  context?: unknown | null;
}
