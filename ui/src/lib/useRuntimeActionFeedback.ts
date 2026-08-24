import { useCallback, useEffect, useRef, type Dispatch, type SetStateAction } from 'react'
import { compactHash } from './format'
import { useI18n } from './i18n'
import type { ManagedRuntimeDeployment, ManagedRuntimeFailure, ManagedRuntimeStatus } from './types'

export type RuntimeActionNotice = { message: string; tone: 'success' | 'error' }

const transientStates = new Set(['starting', 'stopping', 'restarting'])
type AcceptedRuntimeAction = { desiredState: ManagedRuntimeStatus['desired_state']; configHash: string }

export function useRuntimeActionFeedback(status: ManagedRuntimeStatus | undefined, setNotice: Dispatch<SetStateAction<RuntimeActionNotice | null>>) {
  const { t } = useI18n()
  const acceptedAction = useRef<AcceptedRuntimeAction | null>(null)

  useEffect(() => {
    if (acceptedAction.current === null || !status || status.pending || transientStates.has(status.runtime_state)) return
    if (status.last_failure) {
      setNotice({ message: formatRuntimeFailure(status.last_failure, t), tone: 'error' })
      acceptedAction.current = null
      return
    }
    if (status.runtime_state === 'failed' || status.last_error) {
      setNotice({ message: `${t('operationFailed')}\n${status.last_error || status.last_exit || t('failed')}`, tone: 'error' })
      acceptedAction.current = null
      return
    }
    if (acceptedAction.current.desiredState === 'stopped' && ['idle', 'stopped'].includes(status.runtime_state)) {
      setNotice({ message: t('operationDone'), tone: 'success' })
      acceptedAction.current = null
      return
    }
    if (status.runtime_state !== 'running') return
    if (acceptedAction.current.configHash && status.active?.config_hash !== acceptedAction.current.configHash) {
      setNotice({ message: t('runtimeUnexpectedDeployment').replace('{current}', deploymentLabel(status.active)), tone: 'error' })
    } else {
      setNotice({ message: t('operationDone'), tone: 'success' })
    }
    acceptedAction.current = null
  }, [setNotice, status, t])

  return useCallback((accepted: ManagedRuntimeStatus) => {
    acceptedAction.current = {
      desiredState: accepted.desired_state,
      configHash: accepted.active?.config_hash || accepted.target?.config_hash || '',
    }
    setNotice({ message: t('operationAccepted'), tone: 'success' })
  }, [setNotice, t])
}

export function formatRuntimeFailure(failure: ManagedRuntimeFailure, t: ReturnType<typeof useI18n>['t']) {
  const rolledBack = Boolean(failure.rolled_back_to)
  const lines = [t(rolledBack ? 'runtimeFailedRolledBack' : 'runtimeFailedNoRollback')]
  lines.push(t('runtimeFailureStage').replace('{stage}', failure.stage))
  lines.push(t('runtimeFailureError').replace('{error}', failure.error))
  if (failure.failed && failure.rolled_back_to) {
    lines.push(t('runtimeRollback').replace('{failed}', deploymentLabel(failure.failed)).replace('{restored}', deploymentLabel(failure.rolled_back_to)))
  }
  return lines.join('\n')
}

function deploymentLabel(deployment?: ManagedRuntimeDeployment | null) {
  if (!deployment) return '-'
  return `${deployment.exact_reference} · ${compactHash(deployment.config_hash)}`
}
