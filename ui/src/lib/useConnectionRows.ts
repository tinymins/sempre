import { useState } from 'react'
import type { Connection, ConnectionSnapshot } from './types'

export interface ConnectionRowData extends Connection {
  closed: boolean
  downloadSpeed: number | null
  uploadSpeed: number | null
}

export function mergeConnectionSnapshot(previous: ConnectionRowData[], connections: Connection[], elapsedMs: number, keepClosed: boolean): ConnectionRowData[] {
  const previousByID = new Map(previous.map((item) => [item.id, item]))
  const currentIDs = new Set(connections.map((item) => item.id))
  const rows = connections.map((item) => {
    const last = previousByID.get(item.id)
    const canMeasure = last && !last.closed && last.start === item.start && elapsedMs > 0
    return {
      ...item,
      closed: false,
      downloadSpeed: canMeasure ? Math.max(0, item.download - last.download) * 1000 / elapsedMs : null,
      uploadSpeed: canMeasure ? Math.max(0, item.upload - last.upload) * 1000 / elapsedMs : null,
    }
  })
  if (keepClosed) {
    for (const item of previous) {
      if (!currentIDs.has(item.id)) rows.push({ ...item, closed: true, downloadSpeed: null, uploadSpeed: null })
    }
  }
  return rows
}

// Page-local history advances only when React Query receives a successful snapshot.
export function useConnectionRows(snapshot: ConnectionSnapshot | undefined, updatedAt: number, keepClosed: boolean) {
  const [history, setHistory] = useState<{ updatedAt: number; keepClosed: boolean; rows: ConnectionRowData[] }>({ updatedAt: 0, keepClosed, rows: [] })
  let current = history
  if (snapshot && updatedAt !== history.updatedAt) {
    const connections = Array.isArray(snapshot.connections) ? snapshot.connections : []
    current = { updatedAt, keepClosed, rows: mergeConnectionSnapshot(history.rows, connections, updatedAt - history.updatedAt, keepClosed) }
  } else if (keepClosed !== history.keepClosed) {
    current = { ...history, keepClosed, rows: keepClosed ? history.rows : history.rows.filter((item) => !item.closed) }
  }
  if (current !== history) setHistory(current)
  return current.rows
}
