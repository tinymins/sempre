const SESSION_KEY = 'sempre.server.session.v1'

export interface ServerSession {
  token: string
  expiresAt: string
  user: {
    id: string
    email: string
  }
}

interface ErrorBody {
  error?: {
    message?: string
  }
}

export function loadServerSession(): ServerSession | null {
  try {
    const stored = localStorage.getItem(SESSION_KEY)
    if (!stored) return null
    const session = JSON.parse(stored) as ServerSession
    if (!session.token || !session.expiresAt || new Date(session.expiresAt) <= new Date()) {
      saveServerSession(null)
      return null
    }
    return session
  } catch {
    saveServerSession(null)
    return null
  }
}

export function saveServerSession(session: ServerSession | null) {
  if (session) {
    localStorage.setItem(SESSION_KEY, JSON.stringify(session))
    return
  }
  localStorage.removeItem(SESSION_KEY)
}

export async function login(email: string, password: string): Promise<ServerSession> {
  const response = await fetch('/api/v1/auth/login', {
    method: 'POST',
    headers: {
      Accept: 'application/json',
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ email, password }),
  })
  const result = await readResponse<{
    token: string
    expires_at: string
    user: ServerSession['user']
  }>(response)
  return {
    token: result.token,
    expiresAt: result.expires_at,
    user: result.user,
  }
}

export async function verifyServerSession(session: ServerSession) {
  return authenticatedRequest<ServerSession['user']>(session, '/auth/me')
}

export async function logout(session: ServerSession) {
  await authenticatedRequest<void>(session, '/auth/logout', { method: 'DELETE' })
}

async function authenticatedRequest<T>(session: ServerSession, path: string, init: RequestInit = {}) {
  const headers = new Headers(init.headers)
  headers.set('Accept', 'application/json')
  headers.set('Authorization', `Bearer ${session.token}`)
  const response = await fetch(`/api/v1${path}`, { ...init, headers })
  if (response.status === 401) saveServerSession(null)
  return readResponse<T>(response)
}

async function readResponse<T>(response: Response): Promise<T> {
  if (response.ok) {
    if (response.status === 204) return undefined as T
    return response.json() as Promise<T>
  }
  try {
    const body = await response.json() as ErrorBody
    throw new Error(body.error?.message || `HTTP ${response.status}`)
  } catch (error) {
    if (error instanceof Error && !error.message.startsWith('Unexpected')) throw error
    throw new Error(`HTTP ${response.status}`, { cause: error })
  }
}
