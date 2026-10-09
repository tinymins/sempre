import { applyEdits, findNodeAtLocation, parseTree } from 'jsonc-parser'
import { readJsoncObject, type JsonObject } from './jsonc'

export interface RuleSource extends JsonObject {
  name: string
  url: string
  type?: string
  format?: string
}

export function readRuleList(value: string): Record<string, RuleSource[]> | null {
  const { object } = readJsoncObject(value)
  if (!object || Object.values(object).some(items => !Array.isArray(items) || items.some(item =>
    !item || typeof item !== 'object' || Array.isArray(item)
    || typeof item.name !== 'string' || typeof item.url !== 'string'
    || (item.type !== undefined && typeof item.type !== 'string')
    || (item.format !== undefined && typeof item.format !== 'string'),
  ))) return null
  return object as Record<string, RuleSource[]>
}

/** Rename only the property key so group order, comments and source fields remain intact. */
export function renameRuleGroup(value: string, group: string, next: string): string {
  const tree = parseTree(value)
  const key = tree && findNodeAtLocation(tree, [group])?.parent?.children?.[0]
  return key ? applyEdits(value, [{ offset: key.offset, length: key.length, content: JSON.stringify(next) }]) : value
}
