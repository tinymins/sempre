import { act, cleanup, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, expect, it, vi } from 'vitest'
import { ServiceUpdateFlow } from './ServiceUpdateFlow'
import { SessionProvider, useSession } from '../../lib/session'
import { I18nProvider } from '../../lib/i18n'
import { api, saveSession } from '../../lib/api'
import { clearServiceUpdateMarker, serviceUpdateTaskKey, writeServiceUpdateMarker } from '../../lib/useServiceUpdateTask'
import { completeServiceUpdateOnVersionChange } from './ServiceVersionReload'
import type { ServiceUpdateTask } from '../../lib/types'

const session = { baseURL: 'http://sempre.test', token: 'old-session', expiresAt: '2099-01-01T00:00:00Z' }
const task: ServiceUpdateTask = { id: 'update-1', state: 'running', stage: 'installing', current_version: '2.0.0', target_version: '2.0.12', downloaded_bytes: 100, total_bytes: 100, bytes_per_second: 0, started_at: '2026-09-08T00:00:00Z', updated_at: '2026-09-08T00:00:01Z' }
function Background() {
  return <div>{useSession().session ? 'Management background' : 'Login page'}</div>
}
function renderFlow(client: QueryClient) {
  return render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><ServiceUpdateFlow><Background /></ServiceUpdateFlow></SessionProvider></I18nProvider></QueryClientProvider>)
}
afterEach(() => { cleanup(); clearServiceUpdateMarker(); sessionStorage.clear(); localStorage.clear(); vi.unstubAllGlobals() })

it('keeps the current page after session loss and completes from public version information', async () => {
  saveSession(session)
  writeServiceUpdateMarker({ targetVersion: task.target_version, baseURL: session.baseURL, task })
  vi.stubGlobal('fetch', vi.fn(async (_url: string, init?: RequestInit) => {
    expect(new Headers(init?.headers).get('Authorization')).toBe('Bearer old-session')
    return Response.json({ error: { code: 'UNAUTHORIZED', message: 'session expired' } }, { status: 401 })
  }))
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  client.setQueryData(serviceUpdateTaskKey, { task })
  renderFlow(client)
  await act(async () => { await api(session, '/system').catch(() => {}) })
  expect(sessionStorage.getItem('sempre.session.v1')).toBeNull()
  expect(screen.getByText('Management background')).toBeInTheDocument()
  expect(screen.queryByText('Login page')).not.toBeInTheDocument()
  act(() => { completeServiceUpdateOnVersionChange(client, 'v2.0.12', { id: 'new-ui', version: '2.0.12' }) })
  expect(await screen.findByRole('button', { name: 'Sign in again' })).toBeInTheDocument()
  expect(screen.getByText('Management background')).toBeInTheDocument()
  client.clear()
})

it('does not restore an update from browser storage on a new page', () => {
  sessionStorage.setItem('sempre.service-update.v1', JSON.stringify({ targetVersion: task.target_version, baseURL: session.baseURL, task }))
  const fetchMock = vi.fn()
  vi.stubGlobal('fetch', fetchMock)
  const client = new QueryClient()
  renderFlow(client)
  expect(screen.getByText('Login page')).toBeInTheDocument()
  expect(screen.queryByRole('heading', { name: /Updating Sempre|Sempre update complete/ })).not.toBeInTheDocument()
  expect(fetchMock).not.toHaveBeenCalled()
  client.clear()
})
