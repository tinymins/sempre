export interface ServerUser {
  id: string
  name: string
  email: string
  role: string
  settings: Record<string, unknown> | null
}

interface ErrorBody {
  error?: { code?: string; message?: string }
}

export class ServerApiError extends Error {
  constructor(message: string, readonly status: number, options?: ErrorOptions) { super(message, options) }
}

export async function serverRequest<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers)
  headers.set('Accept', 'application/json')
  if (init.body && !(init.body instanceof FormData)) headers.set('Content-Type', 'application/json')
  let response: Response
  try {
    response = await fetch(`/api/v1${path}`, { ...init, headers, credentials: 'same-origin' })
  } catch (reason) {
    throw new Error('无法连接订阅服务，请检查服务状态后重试。', { cause: reason })
  }
  if (response.ok) {
    if (response.status === 204) return undefined as T
    try {
      return await response.json() as T
    } catch (reason) {
      throw new ServerApiError('订阅服务返回了无效响应，请稍后重试。', response.status, { cause: reason })
    }
  }
  let message = `HTTP ${response.status}`
  try {
    const body = await response.json() as ErrorBody
    message = body.error?.message || message
  } catch { /* Keep the HTTP status for non-JSON errors. */ }
  throw new ServerApiError(message, response.status)
}

export async function login(email: string, password: string): Promise<ServerUser> {
  const result = await serverRequest<{ user: ServerUser }>('/auth/login', {
    method: 'POST', body: JSON.stringify({ email, password }),
  })
  return result.user
}

export function authConfig(): Promise<{ allowRegistration: boolean; firstUser: boolean }> {
  return serverRequest('/auth/config')
}

export async function register(name: string, email: string, password: string, invitationCode?: string): Promise<ServerUser> {
  const result = await serverRequest<{ user: ServerUser }>('/auth/register', {
    method: 'POST', body: JSON.stringify({ name, email, password, ...(invitationCode ? { invitationCode } : {}) }),
  })
  return result.user
}

export async function verifyServerSession(): Promise<ServerUser> {
  const result = await serverRequest<{ user: ServerUser }>('/auth/me')
  return result.user
}

export async function logout(): Promise<void> {
  await serverRequest<void>('/auth/logout', { method: 'POST' })
}
