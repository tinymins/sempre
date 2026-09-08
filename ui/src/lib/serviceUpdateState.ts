import type { ServiceUpdateTask } from './types'

export interface ServiceUpdateMarker {
  targetVersion: string
  baseURL?: string
  task?: ServiceUpdateTask
}

let current: ServiceUpdateMarker | null = null

export function readServiceUpdateMarker() {
  return current
}

export function writeServiceUpdateMarker(marker: ServiceUpdateMarker) {
  current = marker
}

export function clearServiceUpdateMarker() {
  current = null
}
