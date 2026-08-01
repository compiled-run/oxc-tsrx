import { parseModule } from '../../packages/tsrx-core-compat/dist/index.js';
import type {
	CodeMapping,
	CompileError,
	MappingData,
	ParseOptions,
	VolarMappingsResult,
} from '../../packages/tsrx-core-compat/dist/types/index.js';
import type * as AST from '../../packages/tsrx-core-compat/dist/types/estree.js';

const mappingData: MappingData = {
	verification: true,
	completion: true,
	semantic: true,
	navigation: true,
	structure: true,
	format: false,
	customData: {},
};

const mapping: CodeMapping = {
	sourceOffsets: [0],
	generatedOffsets: [0],
	lengths: [1],
	generatedLengths: [1],
	data: mappingData,
};

const errors: CompileError[] = [];
const comments: AST.CommentWithLocation[] = [];
const options: ParseOptions = { collect: true, loose: false, errors, comments };
const sourceAst: AST.Program = parseModule('export const value = 1', 'module.tsrx', options);
const result: VolarMappingsResult = {
	code: 'export const value = 1',
	mappings: [mapping],
	cssMappings: [],
	errors,
	sourceAst,
};

void result;
