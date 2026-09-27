import { ServerApiError } from '../server-api'
import { translateCurrent } from '../i18n/provider'
import type { DebugStage } from './diagnostic-types'

interface ErrorBody { error?: { message?: string } }

export async function readDebugStream<Result>(
  path: string,
  body: unknown,
  signal: AbortSignal,
  onStage: (stage: DebugStage) => void,
): Promise<Result> {
  let response: Response
  try {
    response = await fetch(`/api/v1${path}`, {
      method: 'POST',
      headers: { Accept: 'text/event-stream', 'Content-Type': 'application/json' },
      credentials: 'same-origin',
      body: JSON.stringify(body),
      signal,
    })
  } catch (reason) {
    if (signal.aborted) throw reason
    throw new Error(translateCurrent('auth.networkError'), { cause: reason })
  }
  if (!response.ok) {
    let message = `HTTP ${response.status}`
    try {
      const body = await response.json() as ErrorBody
      message = body.error?.message || message
    } catch { /* Keep the HTTP status for non-JSON errors. */ }
    throw new ServerApiError(message, response.status)
  }
  if (!response.body || !response.headers.get('Content-Type')?.includes('text/event-stream')) {
    throw new ServerApiError(translateCurrent('auth.invalidResponse'), response.status)
  }

  const reader = response.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ''
  try {
    while (true) {
      const { value, done } = await reader.read()
      buffer += decoder.decode(value, { stream: !done })
      let boundary = /\r?\n\r?\n/.exec(buffer)
      while (boundary) {
        const frame = buffer.slice(0, boundary.index)
        buffer = buffer.slice(boundary.index + boundary[0].length)
        const event = /^event:\s*(.+)$/m.exec(frame)?.[1]?.trim()
        const data = frame.split(/\r?\n/).filter((line) => line.startsWith('data:')).map((line) => line.slice(5).trimStart()).join('\n')
        if (event && data) {
          let payload: unknown
          try { payload = JSON.parse(data) }
          catch (reason) { throw new ServerApiError(translateCurrent('auth.invalidResponse'), response.status, { cause: reason }) }
          if (signal.aborted) throw signal.reason
          if (event === 'stage') onStage(payload as DebugStage)
          if (event === 'result' || event === 'error') return payload as Result
        }
        boundary = /\r?\n\r?\n/.exec(buffer)
      }
      if (done) throw new ServerApiError(translateCurrent('auth.invalidResponse'), response.status)
    }
  } finally {
    await reader.cancel().catch(() => undefined)
    reader.releaseLock()
  }
}
