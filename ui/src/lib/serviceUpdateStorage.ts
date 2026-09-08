import type { ServiceUpdateTask } from './types'

export const serviceUpdateSessionKey = 'sempre.service-update.v1'

export interface ServiceUpdateMarker {
  targetVersion: string
  baseURL?: string
  task?: ServiceUpdateTask
}

export function readServiceUpdateMarker(): ServiceUpdateMarker | null {
  try {
    const value = JSON.parse(sessionStorage.getItem(serviceUpdateSessionKey) || 'null') as ServiceUpdateMarker | null
    return value?.targetVersion ? value : null
  } catch {
    return null
  }
}

export function writeServiceUpdateMarker(marker: ServiceUpdateMarker) {
  sessionStorage.setItem(serviceUpdateSessionKey, JSON.stringify(marker))
}

export function clearServiceUpdateMarker() {
  sessionStorage.removeItem(serviceUpdateSessionKey)
}
