import { applyEdits, modify, parse, type ParseError } from 'jsonc-parser'

export type JsonObject = Record<string, unknown>

export function readJsoncObject(value: string | null): { object: JsonObject | null; error: boolean } {
  if (!value?.trim()) return { object: {}, error: false }
  const errors: ParseError[] = []
  const parsed: unknown = parse(value, errors, { allowTrailingComma: true })
  if (errors.length > 0 || !parsed || Array.isArray(parsed) || typeof parsed !== 'object') {
    return { object: null, error: true }
  }
  return { object: parsed as JsonObject, error: false }
}

export function objectAt(parent: JsonObject, key: string): JsonObject {
  const value = parent[key]
  return value && !Array.isArray(value) && typeof value === 'object' ? value as JsonObject : {}
}

export function editJsonc(value: string | null, path: (string | number)[], next: unknown): string {
  const text = value?.trim() ? value : '{}'
  const edits = modify(text, path, next, { formattingOptions: { insertSpaces: true, tabSize: 2, eol: '\n' } })
  return applyEdits(text, edits)
}
