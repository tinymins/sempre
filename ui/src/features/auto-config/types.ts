export interface AutoConfigCandidate {
  id: string
  core: string
  reference: string
  configuration_mode: string
  score: number
  reasons: string[]
  warnings?: string[]
  installed: boolean
  selected: boolean
}

export interface AutoConfigCheck {
  id: string
  status: 'pass' | 'info' | 'warning'
  detail?: string
}

export interface AutoConfigReport {
  checked_at: string
  platform: string
  architecture: string
  recommendation?: AutoConfigCandidate
  candidates: AutoConfigCandidate[]
  checks: AutoConfigCheck[]
}

export interface AutoConfigApplyResult {
  recommendation: AutoConfigCandidate
  changes: Array<{ Changed?: boolean; NeedsRestart?: boolean; Message?: string }>
}
