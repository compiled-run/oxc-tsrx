import type { ParseOptions } from "./types/index.js";
import type { Program } from "./types/estree.js";

export function parseModule(source: string, filename?: string, options?: ParseOptions): Program;
export function isEventAttribute(name: string): boolean;
export function normalizeEventName(name: string): string;
