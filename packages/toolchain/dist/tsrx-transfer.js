//#region src/tsrx-transfer.ts
const PROGRAM_TRANSFER_VERSION = 1;
const PROGRAM_BINARY_TRANSFER_MAGIC = 1112691540;
const PROGRAM_BINARY_TRANSFER_VERSION = 1;
const PROGRAM_TRANSFER_MAX_BYTES = 268435456;
const PROGRAM_BINARY_HEADER_WORDS = 12;
const SCALAR_TAG = 0;
const OBJECT_TAG = 1;
const LIST_TAG = 2;
const INLINE_U32_TAG = 3;
const VALUE_INDEX_MASK = 1073741823;
const UNUSED_RANGE = 4294967295;
const COMMON_KEY_FLAG = 2147483648;
const COMMON_KEY_INDEX_MASK = 2147483647;
const TSRX_CORE_COMPAT_DEFAULTS_STRIPPED = Symbol.for("@oxc-tsrx/parser/tsrx-core-compat-defaults-stripped");
const COMMON_KEYS = Object.freeze([
	"body",
	"end",
	"hashbang",
	"sourceType",
	"start",
	"type",
	"async",
	"attributes",
	"declaration",
	"expression",
	"generator",
	"id",
	"metadata",
	"name",
	"params",
	"path",
	"render",
	"children",
	"closingElement",
	"openingElement",
	"selfClosing",
	"value",
	"raw",
	"source",
	"specifiers",
	"kind",
	"local",
	"phase",
	"optional",
	"imported",
	"declarations",
	"init",
	"arguments",
	"callee",
	"computed",
	"key",
	"operator",
	"method",
	"properties",
	"shorthand",
	"left",
	"right",
	"object",
	"property",
	"argument",
	"statementType",
	"prefix",
	"consequent",
	"test",
	"alternate",
	"block",
	"finalizer",
	"handler",
	"param",
	"pending",
	"resetParam",
	"await",
	"empty",
	"index",
	"elements",
	"typeAnnotation",
	"accessibility",
	"declare",
	"members"
]);
function isTsrxBinaryProgram(payload) {
	return payload !== null && typeof payload === "object" && Object.prototype.toString.call(payload.words) === "[object Uint32Array]";
}
function invalid(message) {
	throw new TypeError(`invalid TSRX Program transfer: ${message}`);
}
function applyFix(program, path) {
	if (!Array.isArray(path)) invalid("fix path is not an array");
	let node = program;
	for (const key of path) {
		if (!(typeof key === "string" || Number.isSafeInteger(key) && key >= 0) || node === null || typeof node !== "object" || !Object.hasOwn(node, key)) invalid("fix path does not select a Program node");
		node = node[key];
	}
	if (node === null || typeof node !== "object") invalid("fix path does not end at a Program node");
	if (typeof node.bigint === "string") {
		node.value = BigInt(node.bigint);
		return;
	}
	if (node.regex !== null && typeof node.regex === "object" && typeof node.regex.pattern === "string" && typeof node.regex.flags === "string") {
		try {
			node.value = RegExp(node.regex.pattern, node.regex.flags);
		} catch {}
		return;
	}
	invalid("fix path selects no supported special value");
}
function parseMetadata(metadata, keyCount, scalarCount, fixCount) {
	let decoded;
	try {
		decoded = JSON.parse(metadata);
	} catch {
		invalid("metadata is not valid JSON");
	}
	if (!Array.isArray(decoded) || decoded.length !== 3) invalid("metadata does not have exactly three tables");
	const [keys, scalars, fixes] = decoded;
	if (!Array.isArray(keys) || keys.length !== keyCount) invalid("key metadata count does not match the graph header");
	for (const key of keys) if (typeof key !== "string") invalid("key metadata contains a non-string");
	if (!Array.isArray(scalars) || scalars.length !== scalarCount) invalid("scalar metadata count does not match the graph header");
	for (const scalar of scalars) if (scalar !== null && typeof scalar !== "string" && typeof scalar !== "number" && typeof scalar !== "boolean") invalid("scalar metadata contains a non-scalar");
	if (!Array.isArray(fixes) || fixes.length !== fixCount) invalid("fix metadata count does not match the graph header");
	for (const path of fixes) if (!Array.isArray(path)) invalid("fix metadata contains a non-path");
	return {
		keys,
		scalars,
		fixes
	};
}
function parseBinaryProgram(payload) {
	if (payload === null || typeof payload !== "object" || typeof payload.metadata !== "string" || !isTsrxBinaryProgram(payload)) invalid("binary payload has invalid lanes");
	const { metadata, words } = payload;
	if (words.byteLength > PROGRAM_TRANSFER_MAX_BYTES || Buffer.byteLength(metadata, "utf8") > PROGRAM_TRANSFER_MAX_BYTES - words.byteLength) invalid("binary payload exceeds its bounded capacity");
	if (words.length < PROGRAM_BINARY_HEADER_WORDS) invalid("binary graph header is truncated");
	if (words[0] !== PROGRAM_BINARY_TRANSFER_MAGIC) invalid("binary graph magic does not match");
	if (words[1] !== PROGRAM_BINARY_TRANSFER_VERSION) invalid(`unsupported binary version ${String(words[1])}`);
	if (words[11] !== 0) invalid("binary graph reserved word is nonzero");
	const objectCount = words[2];
	const fieldCount = words[3];
	const listCount = words[4];
	const valueCount = words[5];
	const rootTag = words[6];
	const rootIndex = words[7];
	const keyCount = words[8];
	const scalarCount = words[9];
	const fixCount = words[10];
	const objectOffset = PROGRAM_BINARY_HEADER_WORDS;
	const fieldOffset = objectOffset + objectCount * 2;
	const listOffset = fieldOffset + fieldCount * 2;
	const valueOffset = listOffset + listCount * 2;
	const expectedWords = valueOffset + valueCount;
	if (!Number.isSafeInteger(expectedWords) || words.length !== expectedWords) invalid("binary graph table lengths do not match the header");
	if (rootTag !== OBJECT_TAG || rootIndex >= objectCount) invalid("binary graph root is not a valid Program object");
	const { keys, scalars, fixes } = parseMetadata(metadata, keyCount, scalarCount, fixCount);
	const objects = new Array(objectCount);
	const fieldOwners = new Uint8Array(fieldCount);
	for (let index = 0; index < objectCount; index += 1) {
		const offset = objectOffset + index * 2;
		const start = words[offset];
		const count = words[offset + 1];
		if (start === UNUSED_RANGE) {
			if (count !== 0) invalid("unused binary object has fields");
			continue;
		}
		if (start + count > fieldCount) invalid("binary object field range is truncated");
		for (let field = start; field < start + count; field += 1) {
			if (fieldOwners[field] !== 0) invalid("binary object field ranges overlap");
			fieldOwners[field] = 1;
		}
		objects[index] = {};
	}
	for (const owner of fieldOwners) if (owner !== 1) invalid("binary object fields are unowned");
	const lists = new Array(listCount);
	const valueOwners = new Uint8Array(valueCount);
	for (let index = 0; index < listCount; index += 1) {
		const offset = listOffset + index * 2;
		const start = words[offset];
		const count = words[offset + 1];
		if (start === UNUSED_RANGE) {
			if (count !== 0) invalid("unused binary list has values");
			continue;
		}
		if (start + count > valueCount) invalid("binary list value range is truncated");
		for (let value = start; value < start + count; value += 1) {
			if (valueOwners[value] !== 0) invalid("binary list value ranges overlap");
			valueOwners[value] = 1;
		}
		lists[index] = new Array(count);
	}
	for (const owner of valueOwners) if (owner !== 1) invalid("binary list values are unowned");
	if (objects[rootIndex] === void 0) invalid("binary graph root is unused");
	const owners = new Uint8Array(objectCount + listCount);
	owners[rootIndex] = 1;
	for (let objectIndex = 0; objectIndex < objectCount; objectIndex += 1) {
		const object = objects[objectIndex];
		const rangeOffset = objectOffset + objectIndex * 2;
		const start = words[rangeOffset];
		const count = words[rangeOffset + 1];
		for (let fieldIndex = start; fieldIndex < start + count; fieldIndex += 1) {
			const offset = fieldOffset + fieldIndex * 2;
			const encodedKey = words[offset];
			const key = encodedKey & COMMON_KEY_FLAG ? COMMON_KEYS[encodedKey & COMMON_KEY_INDEX_MASK] : keys[encodedKey];
			if (typeof key !== "string" || !(encodedKey & COMMON_KEY_FLAG) && encodedKey >= keyCount) invalid("binary key index is out of range");
			const packed = words[offset + 1];
			const tag = packed >>> 30;
			const index = packed & VALUE_INDEX_MASK;
			let decoded;
			if (tag === SCALAR_TAG) {
				if (index >= scalarCount) invalid("binary scalar index is out of range");
				decoded = scalars[index];
			} else if (tag === OBJECT_TAG) {
				if (index >= objectCount || objects[index] === void 0) invalid("binary object index is out of range or unused");
				if (owners[index] !== 0) invalid("binary graph contains sharing or a cycle");
				owners[index] = 1;
				decoded = objects[index];
			} else if (tag === LIST_TAG) {
				if (index >= listCount || lists[index] === void 0) invalid("binary list index is out of range or unused");
				const owner = objectCount + index;
				if (owners[owner] !== 0) invalid("binary graph contains sharing or a cycle");
				owners[owner] = 1;
				decoded = lists[index];
			} else if (tag === INLINE_U32_TAG) decoded = index;
			else invalid(`binary value tag ${String(tag)} is invalid`);
			if (key === "__proto__") Object.defineProperty(object, key, {
				configurable: true,
				enumerable: true,
				writable: true,
				value: decoded
			});
			else object[key] = decoded;
		}
	}
	for (let listIndex = 0; listIndex < listCount; listIndex += 1) {
		const list = lists[listIndex];
		if (list === void 0) continue;
		const rangeOffset = listOffset + listIndex * 2;
		const start = words[rangeOffset];
		const count = words[rangeOffset + 1];
		for (let index = 0; index < count; index += 1) {
			const packed = words[valueOffset + start + index];
			const tag = packed >>> 30;
			const valueIndex = packed & VALUE_INDEX_MASK;
			if (tag === SCALAR_TAG) {
				if (valueIndex >= scalarCount) invalid("binary scalar index is out of range");
				list[index] = scalars[valueIndex];
			} else if (tag === OBJECT_TAG) {
				if (valueIndex >= objectCount || objects[valueIndex] === void 0) invalid("binary object index is out of range or unused");
				if (owners[valueIndex] !== 0) invalid("binary graph contains sharing or a cycle");
				owners[valueIndex] = 1;
				list[index] = objects[valueIndex];
			} else if (tag === LIST_TAG) {
				if (valueIndex >= listCount || lists[valueIndex] === void 0) invalid("binary list index is out of range or unused");
				const owner = objectCount + valueIndex;
				if (owners[owner] !== 0) invalid("binary graph contains sharing or a cycle");
				owners[owner] = 1;
				list[index] = lists[valueIndex];
			} else if (tag === INLINE_U32_TAG) list[index] = valueIndex;
			else invalid(`binary value tag ${String(tag)} is invalid`);
		}
	}
	for (let index = 0; index < objectCount; index += 1) if (owners[index] !== Number(objects[index] !== void 0)) invalid("binary graph contains an unreachable container");
	for (let index = 0; index < listCount; index += 1) if (owners[objectCount + index] !== Number(lists[index] !== void 0)) invalid("binary graph contains an unreachable container");
	const program = objects[rootIndex];
	for (const path of fixes) applyFix(program, path);
	return program;
}
function isEmptyArray(value) {
	return Array.isArray(value) && value.length === 0;
}
function omitTsrxCoreCompatDefault(type, key, value) {
	if (isEmptyArray(value) && (key === "decorators" || key === "attributes" && (type === "ExportAllDeclaration" || type === "ExportNamedDeclaration" || type === "ImportDeclaration") || key === "implements" && (type === "ClassDeclaration" || type === "ClassExpression") || key === "extends" && type === "TSInterfaceDeclaration")) return true;
	if (value == null && (key === "accessibility" || key === "directive" || key === "hashbang" || key === "options" || key === "phase" || key === "returnType" || key === "superTypeArguments" || key === "typeAnnotation" || key === "typeArguments" || key === "typeParameters" || type === "RestElement" && key === "value")) return true;
	if (value !== false) return false;
	if (key === "abstract" || key === "const" || key === "declare" || key === "definite" || key === "global" || key === "in" || key === "out" || key === "override" || key === "readonly" || key === "static") return true;
	return key === "optional" && (type === "ArrayPattern" || type === "AssignmentPattern" || type === "Identifier" || type === "MethodDefinition" || type === "ObjectPattern" || type === "Property" || type === "PropertyDefinition" || type === "RestElement" || type === "TSMethodSignature" || type === "TSPropertySignature");
}
function parseTrustedBinaryProgram(payload, stripTsrxCoreCompatDefaults = false) {
	const { metadata, words } = payload;
	const objectCount = words[2];
	const fieldCount = words[3];
	const listCount = words[4];
	const objectOffset = PROGRAM_BINARY_HEADER_WORDS;
	const fieldOffset = objectOffset + objectCount * 2;
	const listOffset = fieldOffset + fieldCount * 2;
	const valueOffset = listOffset + listCount * 2;
	const [keys, scalars, fixes] = JSON.parse(metadata);
	const objects = new Array(objectCount);
	const lists = new Array(listCount);
	for (let index = 0; index < objectCount; index += 1) if (words[objectOffset + index * 2] !== UNUSED_RANGE) objects[index] = {};
	for (let index = 0; index < listCount; index += 1) if (words[listOffset + index * 2] !== UNUSED_RANGE) lists[index] = new Array(words[listOffset + index * 2 + 1]);
	for (let listIndex = 0; listIndex < listCount; listIndex += 1) {
		if (lists[listIndex] === void 0) continue;
		const start = words[listOffset + listIndex * 2];
		const count = words[listOffset + listIndex * 2 + 1];
		const list = lists[listIndex];
		for (let index = 0; index < count; index += 1) {
			const packed = words[valueOffset + start + index];
			const tag = packed >>> 30;
			const valueIndex = packed & VALUE_INDEX_MASK;
			list[index] = tag === SCALAR_TAG ? scalars[valueIndex] : tag === OBJECT_TAG ? objects[valueIndex] : tag === LIST_TAG ? lists[valueIndex] : valueIndex;
		}
	}
	for (let objectIndex = 0; objectIndex < objectCount; objectIndex += 1) {
		if (objects[objectIndex] === void 0) continue;
		const start = words[objectOffset + objectIndex * 2];
		const count = words[objectOffset + objectIndex * 2 + 1];
		const object = objects[objectIndex];
		let type;
		if (stripTsrxCoreCompatDefaults) for (let index = 0; index < count; index += 1) {
			const offset = fieldOffset + (start + index) * 2;
			const encodedKey = words[offset];
			if ((encodedKey & COMMON_KEY_FLAG ? COMMON_KEYS[encodedKey & COMMON_KEY_INDEX_MASK] : keys[encodedKey]) !== "type") continue;
			const packed = words[offset + 1];
			if (packed >>> 30 === SCALAR_TAG) type = scalars[packed & VALUE_INDEX_MASK];
			break;
		}
		for (let index = 0; index < count; index += 1) {
			const offset = fieldOffset + (start + index) * 2;
			const packed = words[offset + 1];
			const tag = packed >>> 30;
			const valueIndex = packed & VALUE_INDEX_MASK;
			const decoded = tag === SCALAR_TAG ? scalars[valueIndex] : tag === OBJECT_TAG ? objects[valueIndex] : tag === LIST_TAG ? lists[valueIndex] : valueIndex;
			const encodedKey = words[offset];
			const key = encodedKey & COMMON_KEY_FLAG ? COMMON_KEYS[encodedKey & COMMON_KEY_INDEX_MASK] : keys[encodedKey];
			if (stripTsrxCoreCompatDefaults && omitTsrxCoreCompatDefault(type, key, decoded)) continue;
			object[key] = decoded;
		}
	}
	const program = objects[words[7]];
	for (const path of fixes) applyFix(program, path);
	if (stripTsrxCoreCompatDefaults) Object.defineProperty(program, TSRX_CORE_COMPAT_DEFAULTS_STRIPPED, {
		configurable: true,
		value: true
	});
	return program;
}
function parseTrustedTsrxProgram(payload, stripTsrxCoreCompatDefaults = false) {
	if (isTsrxBinaryProgram(payload)) return parseTrustedBinaryProgram(payload, stripTsrxCoreCompatDefaults);
	return parseTsrxProgram(payload);
}
function parseTsrxProgram(payload) {
	if (payload === null) return null;
	if (typeof payload !== "string") return parseBinaryProgram(payload);
	const envelope = JSON.parse(payload);
	if (envelope === null || typeof envelope !== "object") invalid("envelope is not an object");
	if (envelope.version !== PROGRAM_TRANSFER_VERSION) invalid(`unsupported version ${String(envelope.version)}`);
	if (envelope.node === null || typeof envelope.node !== "object") invalid("Program is not an object");
	if (!Array.isArray(envelope.fixes)) invalid("fixes is not an array");
	for (const path of envelope.fixes) applyFix(envelope.node, path);
	return envelope.node;
}
//#endregion
export { isTsrxBinaryProgram, parseTrustedTsrxProgram, parseTsrxProgram };
