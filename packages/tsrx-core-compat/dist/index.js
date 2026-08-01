import { createTsrxCoreCompat } from "./facade.js";
import * as parser from "oxc-tsrx/parser";
//#region src/index.ts
const facade = createTsrxCoreCompat(parser);
const parseModule = facade.parseModule;
const isEventAttribute = facade.isEventAttribute;
const normalizeEventName = facade.normalizeEventName;
//#endregion
export { isEventAttribute, normalizeEventName, parseModule };
