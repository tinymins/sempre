import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ServerApp } from './ServerApp'
import { newServerProfile } from './server-api'

function response(body: unknown) {
  return new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } })
}

describe('ServerApp', () => {
  beforeEach(() => {
    window.location.hash = '#/server/subscriptions/profile-1'
    localStorage.setItem('sempre.server.session.v1', JSON.stringify({
      token: 'server-token', expiresAt: '2099-01-01T00:00:00Z', user: { id: 'user-1', email: 'viewer@example.com' },
    }))
    const document = newServerProfile('Team profile')
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url === '/api/v1/profiles/profile-1') return response({
        id: 'profile-1', owner_id: 'owner-1', revision: 5, name: 'Team profile', document, role: 'viewer', updated_at: '2026-08-24T00:00:00Z',
      })
      if (url === '/api/v1/targets') return response([{ format: 'sing-box-v13', version: '13', platform: 'default' }])
      if (url === '/api/v1/custom-nodes') return response([])
      if (url === '/api/v1/profiles/profile-1/stats') return response({ total_accesses: 0, today_accesses: 0, by_target: [], recent_accesses: [] })
      throw new Error(`Unexpected request: ${url}`)
    }))
  })

  afterEach(() => {
    cleanup()
    localStorage.removeItem('sempre.server.session.v1')
    window.location.hash = ''
    vi.unstubAllGlobals()
  })

  it('opens the manifest edit route as a read-only page for viewers', async () => {
    render(<ServerApp />)
    expect(await screen.findByRole('heading', { name: 'Team profile' })).toBeInTheDocument()
    expect(screen.getByText('viewer')).toBeInTheDocument()
    expect(screen.getByText('This shared profile is read-only.')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument()
    expect(screen.queryByText('Custom node library')).not.toBeInTheDocument()
    expect(screen.getByText(/"name": "Team profile"/)).toBeInTheDocument()
  })
})
