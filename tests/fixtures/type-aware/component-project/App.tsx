/// <reference path="./jsx.d.ts" />

import { View } from "./View.tsrx";

declare function load(): Promise<void>;

load();
export const app = <View label="ready" />;
export const rendered: JSX.Element = View({ label: "ready" });
