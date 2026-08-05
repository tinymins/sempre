import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { api, loadSession, saveSession, subscribeToSessionInvalidation } from './api'
import type { Session } from './types'

const oldSession: Session = {
  baseURL: 'http://sempre.test',
  token: 'old-session',
  expiresAt: '2099-01-01T00:00:00Z',
}

describe('authenticated API session handling', () => {
  beforeEach(() => sessionStorage.clear())
  afterEach(() => vi.unstubAllGlobals())

  it('invalidates the current session after a 401 response', async () => {
    saveSession(oldSession)
    const invalidated = vi.fn()
    const unsubscribe = subscribeToSessionInvalidation(invalidated)
    vi.stubGlobal('fetch', vi.fn(async () => unauthorizedResponse()))

    await expect(api(oldSession, '/system')).rejects.toMatchObject({ status: 401 })

    expect(loadSession()).toBeNull()
    expect(invalidated).toHaveBeenCalledOnce()
    unsubscribe()
  })

  it('does not clear a newer session when an old request returns 401 late', async () => {
    saveSession(oldSession)
    let finishRequest: (response: Response) => void = () => undefined
    vi.stubGlobal('fetch', vi.fn(() => new Promise<Response>((resolve) => { finishRequest = resolve })))

    const request = api(oldSession, '/system')
    const newSession = { ...oldSession, token: 'new-session' }
    saveSession(newSession)
    finishRequest(unauthorizedResponse())

    await expect(request).rejects.toMatchObject({ status: 401 })
    expect(loadSession()).toEqual(newSession)
  })
})

function unauthorizedResponse() {
  return Response.json({ error: { code: 'UNAUTHORIZED', message: 'a valid administrator session is required' } }, { status: 401 })
}
