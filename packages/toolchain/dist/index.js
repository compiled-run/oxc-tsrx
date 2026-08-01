//#region src/index.ts
const extensions = Object.freeze([".tsrx"]);
const capabilities = Object.freeze([
	"parser",
	"lint",
	"format",
	"languageServer"
]);
const toolchain = Object.freeze({
	name: "oxc-tsrx",
	language: "tsrx",
	extensions,
	capabilities
});
//#endregion
export { toolchain };
