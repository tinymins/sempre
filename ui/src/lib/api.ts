import type { ApiErrorBody, RuntimeEvent, Session } from './types'

const SESSION_KEY = 'sempre.session.v1'
const sessionInvalidationListeners = new Set<() => void>()

export class ApiError extends Error {
  status: number
  code: string
  details?: unknown

  constructor(status: number, body: ApiErrorBody) {
    super(body.error.message)
    this.status = status
    this.code = body.error.code
    this.details = body.error.details
  }
}

export function normalizeBaseURL(value: string) {
  const input = value.trim().replace(/\/+$/, '')
  const parsed = new URL(input)
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') throw new Error('Address must use HTTP or HTTPS')
  return parsed.origin + parsed.pathname.replace(/\/$/, '')
}

export function loadSession(): Session | null {
  try {
    const value = sessionStorage.getItem(SESSION_KEY)
    if (!value) return null
    const session = JSON.parse(value) as Session
    if (!session.baseURL || !session.token || new Date(session.expiresAt) <= new Date()) {
      sessionStorage.removeItem(SESSION_KEY)
      return null
    }
    return session
  } catch {
    return null
  }
}

export function saveSession(session: Session | null) {
  if (session) sessionStorage.setItem(SESSION_KEY, JSON.stringify(session))
  else sessionStorage.removeItem(SESSION_KEY)
}

export function subscribeToSessionInvalidation(listener: () => void) {
  sessionInvalidationListeners.add(listener)
  return () => {
    sessionInvalidationListeners.delete(listener)
  }
}

function invalidateSession(session: Session) {
  const stored = sessionStorage.getItem(SESSION_KEY)
  if (!stored) return
  try {
    const current = JSON.parse(stored) as Session
    if (current.baseURL !== session.baseURL || current.token !== session.token) return
  } catch {
    return
  }
  saveSession(null)
  sessionInvalidationListeners.forEach((listener) => listener())
}

async function parseResponse<T>(response: Response, session?: Session): Promise<T> {
  if (response.ok) {
    if (response.status === 204) return undefined as T
    return (await response.json()) as T
  }
  if (response.status === 401 && session) invalidateSession(session)
  let body: ApiErrorBody
  try {
    body = (await response.json()) as ApiErrorBody
  } catch {
    body = { error: { code: 'HTTP_ERROR', message: `HTTP ${response.status}` } }
  }
  throw new ApiError(response.status, body)
}

export async function login(baseURL: string, password: string): Promise<Session> {
  const normalized = normalizeBaseURL(baseURL)
  const response = await fetch(`${normalized}/api/v1/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ password }),
  })
  const result = await parseResponse<{ token: string; expires_at: string; warning?: string }>(response)
  return { baseURL: normalized, token: result.token, expiresAt: result.expires_at, warning: result.warning }
}

export async function api<T>(session: Session, path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers)
  headers.set('Accept', 'application/json')
  headers.set('Authorization', `Bearer ${session.token}`)
  if (init.body && !(init.body instanceof Blob) && !headers.has('Content-Type')) headers.set('Content-Type', 'application/json')
  const response = await fetch(`${session.baseURL}/api/v1${path}`, { ...init, headers })
  return parseResponse<T>(response, session)
}

export async function uploadUI(session: Session, file: File, sha256 = '') {
  const response = await fetch(`${session.baseURL}/api/v1/ui/upload?sha256=${encodeURIComponent(sha256)}`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${session.token}`,
      'Content-Type': 'application/zip',
      'X-Sempre-UI-Name': file.name,
    },
    body: file,
  })
  return parseResponse(response, session)
}

export async function streamEvents(
  session: Session,
  topics: string[],
  onEvent: (event: RuntimeEvent) => void,
  signal: AbortSignal,
) {
  const response = await fetch(
    `${session.baseURL}/api/v1/runtime/events?topics=${encodeURIComponent(topics.join(','))}`,
    { headers: { Authorization: `Bearer ${session.token}`, Accept: 'text/event-stream' }, signal },
  )
  if (!response.ok) await parseResponse(response, session)
  if (!response.body) throw new Error('Streaming response has no body')
  const reader = response.body.pipeThrough(new TextDecoderStream()).getReader()
  let buffer = ''
  while (!signal.aborted) {
    const { value, done } = await reader.read()
    if (done) return
    buffer += value
    let boundary = buffer.indexOf('\n\n')
    while (boundary >= 0) {
      const block = buffer.slice(0, boundary)
      buffer = buffer.slice(boundary + 2)
      const data = block
        .split('\n')
        .filter((line) => line.startsWith('data:'))
        .map((line) => line.slice(5).trim())
        .join('\n')
      if (data) onEvent(JSON.parse(data) as RuntimeEvent)
      boundary = buffer.indexOf('\n\n')
    }
  }
}

export async function streamRequest(
  session: Session,
  path: string,
  body: unknown,
  onEvent: (event: string, data: unknown) => void,
  signal?: AbortSignal,
) {
  const response = await fetch(`${session.baseURL}/api/v1${path}`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${session.token}`, Accept: 'text/event-stream', 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal,
  })
  if (!response.ok) await parseResponse(response, session)
  if (!response.body) throw new Error('Streaming response has no body')
  const reader = response.body.pipeThrough(new TextDecoderStream()).getReader()
  let buffer = ''
  for (;;) {
    const { value, done } = await reader.read()
    if (done) return
    buffer += value
    let boundary = buffer.indexOf('\n\n')
    while (boundary >= 0) {
      const block = buffer.slice(0, boundary)
      buffer = buffer.slice(boundary + 2)
      let event = 'message'
      const data: string[] = []
      for (const line of block.split('\n')) {
        if (line.startsWith('event:')) event = line.slice(6).trim()
        if (line.startsWith('data:')) data.push(line.slice(5).trim())
      }
      if (data.length) onEvent(event, JSON.parse(data.join('\n')))
      boundary = buffer.indexOf('\n\n')
    }
  }
}
