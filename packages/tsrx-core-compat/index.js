import * as parser from "@oxc-tsrx/parser";
import { createTsrxCoreCompat } from "./facade.js";

const facade = createTsrxCoreCompat(parser);

export const parseModule = facade.parseModule;
export const isEventAttribute = facade.isEventAttribute;
export const normalizeEventName = facade.normalizeEventName;
