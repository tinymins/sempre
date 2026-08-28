export interface AutoConfigCandidate {
  id: string
  core: string
  reference: string
  configuration_mode: string
  eligible: boolean
  score: number | null
  score_breakdown: Array<{ id: string; points: number; maximum: number }>
  matched_requirements: string[]
  reasons: string[]
  warnings?: string[]
  blockers?: string[]
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
  policy_version: string
  requirements: { required_features: string[]; required_protocols: string[] }
  recommendation?: AutoConfigCandidate
  candidates: AutoConfigCandidate[]
  checks: AutoConfigCheck[]
}

export interface AutoConfigApplyResult {
  recommendation: AutoConfigCandidate
  changes: Array<{ Changed?: boolean; NeedsRestart?: boolean; Message?: string }>
}
