import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { App } from './App'
import { loadServerSession, saveServerSession, type ServerSession } from './server-api'

const session: ServerSession = {
  token: 'token-1',
  expiresAt: '2099-01-01T00:00:00Z',
  user: { id: 'user-1', email: 'owner@example.com' },
}

describe('server app authentication shell', () => {
  beforeEach(() => {
    localStorage.clear()
    window.location.hash = '#/subscriptions'
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('guards authenticated routes with the login screen', () => {
    render(<App />)

    expect(screen.getByRole('heading', { name: 'Sempre Server' })).toBeInTheDocument()
    expect(screen.getByLabelText('邮箱')).toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: '订阅' })).not.toBeInTheDocument()
  })

  it('restores and verifies a saved session', async () => {
    saveServerSession(session)
    vi.mocked(fetch).mockResolvedValue(jsonResponse(session.user))

    render(<App />)

    expect(await screen.findByRole('heading', { name: '订阅' })).toBeInTheDocument()
    expect(fetch).toHaveBeenCalledWith('/api/v1/auth/me', expect.objectContaining({
      headers: expect.any(Headers),
    }))
    const headers = vi.mocked(fetch).mock.calls[0]?.[1]?.headers as Headers
    expect(headers.get('Authorization')).toBe('Bearer token-1')
  })

  it('enters the subscriptions shell after login', async () => {
    vi.mocked(fetch).mockResolvedValue(jsonResponse({
      token: session.token,
      expires_at: session.expiresAt,
      user: session.user,
    }))
    render(<App />)

    fireEvent.change(screen.getByLabelText('邮箱'), { target: { value: 'owner@example.com' } })
    fireEvent.change(screen.getByLabelText('密码'), { target: { value: 'correct horse battery staple' } })
    fireEvent.click(screen.getByRole('button', { name: '登录' }))

    expect(await screen.findByRole('heading', { name: '订阅' })).toBeInTheDocument()
    expect(loadServerSession()).toEqual(session)
  })

  it('clears an unauthorized saved session and returns to login', async () => {
    saveServerSession(session)
    vi.mocked(fetch).mockResolvedValue(jsonResponse({ error: { message: 'Unauthorized' } }, 401))

    render(<App />)

    expect(await screen.findByLabelText('邮箱')).toBeInTheDocument()
    await waitFor(() => expect(loadServerSession()).toBeNull())
  })
})

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}
