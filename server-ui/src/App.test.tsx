import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { App } from './App'

const user = { id: 'user-1', name: 'Owner', email: 'owner@example.com', role: 'user', settings: {} }

describe('server app authentication shell', () => {
  beforeEach(() => {
    window.location.hash = '#/subscriptions'
    Object.defineProperty(navigator, 'languages', { configurable: true, value: ['zh-CN'] })
    Object.defineProperty(navigator, 'language', { configurable: true, value: 'zh-CN' })
    vi.stubGlobal('fetch', vi.fn())
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it('enters the subscriptions page after login', async () => {
    vi.mocked(fetch).mockImplementation((input) => {
      const url = String(input)
      if (url.endsWith('/auth/me')) return Promise.resolve(jsonResponse({ error: { message: 'Unauthorized' } }, 401))
      return Promise.resolve(jsonResponse(url.endsWith('/auth/login') ? { user } : []))
    })
    render(<App />)

    fireEvent.change(await screen.findByLabelText('邮箱'), { target: { value: 'owner@example.com' } })
    fireEvent.change(screen.getByLabelText('密码'), { target: { value: 'correct horse battery staple' } })
    fireEvent.click(screen.getByRole('button', { name: '登录' }))

    expect(await screen.findByRole('heading', { name: '配置集' })).toBeInTheDocument()
    expect(fetch).toHaveBeenCalledWith('/api/v1/auth/login', expect.objectContaining({ method: 'POST', credentials: 'same-origin' }))
  })

  it('returns to login when a cookie session is unauthorized', async () => {
    vi.mocked(fetch).mockResolvedValue(jsonResponse({ error: { message: 'Unauthorized' } }, 401))
    render(<App />)

    expect(await screen.findByLabelText('邮箱')).toBeInTheDocument()
    await waitFor(() => expect(screen.queryByRole('heading', { name: '配置集' })).not.toBeInTheDocument())
  })
})

function jsonResponse(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } })
}
