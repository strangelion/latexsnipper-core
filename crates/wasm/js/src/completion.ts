import type { ApiEnvelopeV3 } from "./types.js";

export interface FormulaSourceRange { start: number; end: number }
export type FormulaSlot = "formula" | "group" | "fraction-numerator" | "fraction-denominator"
  | "fraction-alignment" | "binomial-upper" | "binomial-lower" | "radical-index" | "radical-body"
  | "superscript" | "subscript" | "math-argument" | "text" | "environment-name" | "array-column-spec" | "matrix-cell";
export type FormulaEditKind = { kind: "root" | "text" | "comment" | "group" | "argument" | "separator" }
  | { kind: "command" | "environment"; name: string } | { kind: "script"; base: number | null }
  | { kind: "cell"; row: number; column: number };
export interface FormulaEditNode {
  parent: number | null; kind: FormulaEditKind; range: FormulaSourceRange;
  content: FormulaSourceRange; slot: FormulaSlot | null; closed: boolean;
}
export interface FormulaCompletionSnapshot {
  schemaVersion: number; sourceSha256: string; revision: string; cursor: number;
}
export interface FormulaCompletionContext extends FormulaCompletionSnapshot {
  slot: FormulaSlot; ownerPath: number[]; activeEnvironment: string | null;
  cell: { environment: string; row: number; column: number } | null;
  inComment: boolean; inText: boolean;
  commandPrefix: { replace: FormulaSourceRange; prefix: string } | null;
  nodes: FormulaEditNode[]; diagnostics: { code: string; range: FormulaSourceRange }[];
}
export interface FormulaCompletionCandidate {
  id: string; label: string; source: string; slot: FormulaSlot;
  replace: FormulaSourceRange; insert: string; tabStops: FormulaSourceRange[];
}
export interface FormulaCompletionReply {
  snapshot: FormulaCompletionSnapshot;
  completion: { context: FormulaCompletionContext; candidates: FormulaCompletionCandidate[] };
}
export interface AppliedFormulaCompletion { source: string; cursor: number; tabStops: FormulaSourceRange[] }
export interface FormulaCompletionRequest { source: string; inputFormat: "latex"; cursor: number; revision: string }
export interface FormulaCompletionApplyRequest extends FormulaCompletionRequest { snapshot: FormulaCompletionSnapshot; candidateId: string }
export interface FormulaCompletionCapabilities {
  schemaVersion: number; inputFormats: ["latex"]; offsetEncoding: "utf-8-bytes";
  revisionEncoding: "canonical-decimal-u64-string"; maxInputBytes: number; maxNodes: number;
  maxDepthExclusive: number; maxCandidates: number; maxRequestBytes: number; maxResultDataBytes: number;
}
export interface WasmFormulaCompletionApi {
  formula_completion_capabilities_v3(): ApiEnvelopeV3<FormulaCompletionCapabilities>;
  complete_formula_v3(requestJson: string): ApiEnvelopeV3<FormulaCompletionReply>;
  apply_formula_completion_v3(requestJson: string): ApiEnvelopeV3<AppliedFormulaCompletion>;
}
export const MAX_COMPLETION_ENVELOPE_BYTES = 1024 * 1024 + 64 * 1024;

export interface FormulaDefinitionScope {
  userSha256: string; projectSha256: string | null; documentSha256: string | null; generation: string;
}
export interface FormulaCommandDefinition {
  id: string; revision: string; userSha256: string; projectSha256: string | null;
  command: string; arguments: FormulaDefinitionArgument[];
}
export type FormulaDefinitionArgument = "math" | "text" | "optional-math" | "optional-text";
export interface FormulaDefinitionCatalog { scope: FormulaDefinitionScope; definitions: FormulaCommandDefinition[] }
export interface ScopedFormulaSnapshot extends FormulaCompletionSnapshot { schemaVersion: 2; definitionsSha256: string }
export interface ScopedFormulaCandidate { edit: FormulaCompletionCandidate; definition: { id: string; revision: string } | null }
export interface ScopedFormulaCompletion {
  snapshot: ScopedFormulaSnapshot; context: FormulaCompletionContext; candidates: ScopedFormulaCandidate[];
}
export interface ScopedFormulaCompletionRequest extends FormulaCompletionRequest { catalog: FormulaDefinitionCatalog }
export interface ScopedFormulaCompletionApplyRequest extends ScopedFormulaCompletionRequest { snapshot: ScopedFormulaSnapshot; candidateId: string }
export interface ScopedFormulaCompletionCapabilities extends FormulaCompletionCapabilities {
  schemaVersion: 2; scopeIdentityEncoding: "opaque-lowercase-sha256";
  definitionRevisionEncoding: "canonical-decimal-u64-string"; generationEncoding: "canonical-decimal-u64-string";
  maxDefinitions: number; maxDefinitionArguments: number; macroExpansion: false; persistence: false;
  /** Older Core builds omit these; feature-detect before sending optional kinds. */
  argumentKinds?: FormulaDefinitionArgument[]; maxLeadingOptionalArguments?: number;
}
export interface WasmScopedFormulaCompletionApi {
  formula_scoped_completion_capabilities_v3(): ApiEnvelopeV3<ScopedFormulaCompletionCapabilities>;
  complete_formula_scoped_v3(requestJson: string): ApiEnvelopeV3<ScopedFormulaCompletion>;
  apply_formula_completion_scoped_v3(requestJson: string): ApiEnvelopeV3<AppliedFormulaCompletion>;
}
export const MAX_COMPLETION_CATALOG_BYTES = 64 * 1024;
export function completeFormulaScoped(api: WasmScopedFormulaCompletionApi, request: ScopedFormulaCompletionRequest): ApiEnvelopeV3<ScopedFormulaCompletion> {
  return api.complete_formula_scoped_v3(JSON.stringify(request));
}
export function applyFormulaCompletionScoped(api: WasmScopedFormulaCompletionApi, request: ScopedFormulaCompletionApplyRequest): ApiEnvelopeV3<AppliedFormulaCompletion> {
  return api.apply_formula_completion_scoped_v3(JSON.stringify(request));
}
export function formulaScopedCompletionCapabilities(api: WasmScopedFormulaCompletionApi): ApiEnvelopeV3<ScopedFormulaCompletionCapabilities> {
  return api.formula_scoped_completion_capabilities_v3();
}

/** Synchronous exports: use WasmWorkerClient for interactive hosts. */
export function completeFormula(api: WasmFormulaCompletionApi, request: FormulaCompletionRequest): ApiEnvelopeV3<FormulaCompletionReply> {
  return api.complete_formula_v3(JSON.stringify(request));
}
export function applyFormulaCompletion(api: WasmFormulaCompletionApi, request: FormulaCompletionApplyRequest): ApiEnvelopeV3<AppliedFormulaCompletion> {
  return api.apply_formula_completion_v3(JSON.stringify(request));
}
export function formulaCompletionCapabilities(api: WasmFormulaCompletionApi): ApiEnvelopeV3<FormulaCompletionCapabilities> { return api.formula_completion_capabilities_v3(); }

function wellFormed(source: string): void {
  for (let index = 0; index < source.length; index += 1) {
    const unit = source.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = source.charCodeAt(++index);
      if (!(next >= 0xdc00 && next <= 0xdfff)) throw new RangeError("Unpaired UTF-16 surrogate");
    } else if (unit >= 0xdc00 && unit <= 0xdfff) throw new RangeError("Unpaired UTF-16 surrogate");
  }
}
export function utf16ToFormulaByteOffset(source: string, offset: number): number {
  wellFormed(source);
  if (!Number.isSafeInteger(offset) || offset < 0 || offset > source.length ||
      (offset < source.length && source.charCodeAt(offset) >= 0xdc00 && source.charCodeAt(offset) <= 0xdfff)) throw new RangeError("Offset must be a UTF-16 code point boundary");
  return new TextEncoder().encode(source.slice(0, offset)).byteLength;
}
export function formulaByteOffsetToUtf16(source: string, offset: number): number {
  wellFormed(source);
  if (!Number.isSafeInteger(offset) || offset < 0) throw new RangeError("Invalid UTF-8 byte offset");
  let bytes = 0;
  let units = 0;
  for (const character of source) {
    if (bytes === offset) return units;
    const point = character.codePointAt(0)!;
    bytes += point <= 0x7f ? 1 : point <= 0x7ff ? 2 : point <= 0xffff ? 3 : 4;
    units += character.length;
  }
  if (bytes === offset) return units;
  throw new RangeError("Offset must be a UTF-8 code point boundary");
}
function record(value: unknown): value is Record<string, unknown> { return typeof value === "object" && value !== null && !Array.isArray(value); }
function closed(value: Record<string, unknown>, keys: string[]): boolean { return Object.keys(value).every((key) => keys.includes(key)); }
function revision(value: unknown): boolean {
  return typeof value === "string" && value.length > 0 && value.length <= 20 && !/[^0-9]/.test(value) &&
    (value === "0" || value[0] !== "0") && (value.length < 20 || value <= "18446744073709551615");
}
/** Validate in both host and worker. Core remains the authoritative parser. */
export function completionInputError(input: unknown, apply: boolean): string | undefined { return completionError(input, apply, false); }
export function scopedCompletionInputError(input: unknown, apply: boolean): string | undefined { return completionError(input, apply, true); }
function hash(value: unknown): boolean { return typeof value === "string" && value.length === 64 && !/[^a-f0-9]/.test(value); }
function optionalHash(value: unknown): boolean { return value === null || value === undefined || hash(value); }
function catalogError(value: unknown): string | undefined {
  if (!record(value) || !closed(value, ["scope", "definitions"]) || !record(value.scope) ||
      !closed(value.scope, ["userSha256", "projectSha256", "documentSha256", "generation"]) ||
      !hash(value.scope.userSha256) || !optionalHash(value.scope.projectSha256) || !optionalHash(value.scope.documentSha256) ||
      !revision(value.scope.generation) || !Array.isArray(value.definitions) || value.definitions.length > 64) return "Invalid closed completion catalog";
  // Match transport shape only. Core filters foreign scopes before inspecting
  // private names/signatures and remains authoritative for active declarations.
  for (const entry of value.definitions) {
    if (!record(entry) || !closed(entry, ["id", "revision", "userSha256", "projectSha256", "command", "arguments"]) ||
        typeof entry.id !== "string" || !revision(entry.revision) || typeof entry.userSha256 !== "string" ||
        !(entry.projectSha256 === null || entry.projectSha256 === undefined || typeof entry.projectSha256 === "string") || typeof entry.command !== "string" ||
        !Array.isArray(entry.arguments) || !entry.arguments.every((arg) => arg === "math" || arg === "text" || arg === "optional-math" || arg === "optional-text")) return "Invalid closed completion definition";
  }
  try { if (new TextEncoder().encode(JSON.stringify(value)).byteLength > MAX_COMPLETION_CATALOG_BYTES) return "Completion catalog exceeds 64 KiB JSON"; }
  catch { return "Completion catalog must be JSON-serializable"; }
  return undefined;
}
function completionError(input: unknown, apply: boolean, scoped: boolean): string | undefined {
  if (!record(input) || !closed(input, ["source", "inputFormat", "cursor", "revision", ...(scoped ? ["catalog"] : []), ...(apply ? ["snapshot", "candidateId"] : [])]) ||
      typeof input.source !== "string" || input.inputFormat !== "latex" || !revision(input.revision)) return "Invalid closed LaTeX completion request";
  if (input.source.length > 65536 || new TextEncoder().encode(input.source).byteLength > 65536) return "Completion source exceeds 64 KiB UTF-8";
  try { formulaByteOffsetToUtf16(input.source, input.cursor as number); } catch { return "Invalid completion source or UTF-8 cursor"; }
  if (scoped) { const cause = catalogError(input.catalog); if (cause) return cause; }
  if (apply) {
    const snapshot = input.snapshot;
    if (!record(snapshot) || !closed(snapshot, ["schemaVersion", "sourceSha256", "revision", "cursor", ...(scoped ? ["definitionsSha256"] : [])]) || snapshot.schemaVersion !== (scoped ? 2 : 1) ||
        (scoped && !hash(snapshot.definitionsSha256)) ||
        !revision(snapshot.revision) || typeof snapshot.sourceSha256 !== "string" || snapshot.sourceSha256.length !== 64 ||
        !/^[a-f0-9]{64}$/.test(snapshot.sourceSha256) || !Number.isSafeInteger(snapshot.cursor) || (snapshot.cursor as number) < 0 ||
        (snapshot.cursor as number) > 65536 || typeof input.candidateId !== "string" || input.candidateId.length === 0 || input.candidateId.length > 256) return "Invalid closed completion snapshot or candidate ID";
  }
  return undefined;
}
