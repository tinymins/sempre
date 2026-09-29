import type { EditorSource } from './sources'

export const inheritedFields = {
  ruleList: { flag: 'useSystemRuleList', label: 'editor.tabRules' },
  group: { flag: 'useSystemGroup', label: 'editor.tabGroup' },
  filter: { flag: 'useSystemFilter', label: 'editor.filter' },
  customConfig: { flag: 'useSystemCustomConfig', label: 'editor.tabCustom' },
  dnsConfig: { flag: 'useSystemDnsConfig', label: 'editor.tabDns' },
} as const

export type InheritedField = keyof typeof inheritedFields
export type InheritFlag = typeof inheritedFields[InheritedField]['flag']
export type ConfigDefaults = Record<InheritedField, string>
export type ConfigDraft = Record<InheritedField, string | null> & Record<InheritFlag, boolean> & {
  privateAccessConfig: string | null
  servers: string | null
  selectedCustomNodeIds: string[]
}

export interface EditorDraft extends ConfigDraft {
  remark: string | null
  logLevel: 'off' | 'error' | 'warn' | 'info' | 'debug'
  subscribeItems: EditorSource[]
}

/** Inherited fields have one effective setting, with no inactive custom draft. */
export function clearInheritedValues<T extends Pick<ConfigDraft, InheritedField | InheritFlag>>(value: T): T {
  const next = { ...value }
  for (const field of Object.keys(inheritedFields) as InheritedField[]) {
    if (next[inheritedFields[field].flag]) next[field] = null
  }
  return next
}
