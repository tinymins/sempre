import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, expect, it, vi } from 'vitest'
import { ServiceUpdateFlow } from './ServiceUpdateFlow'
import { SessionProvider, useSession } from '../../lib/session'
import { I18nProvider } from '../../lib/i18n'
import { api, saveSession } from '../../lib/api'
import { serviceUpdateTaskKey, writeServiceUpdateMarker } from '../../lib/useServiceUpdateTask'
import type { ServiceUpdateTask } from '../../lib/types'

const session = { baseURL: 'http://sempre.test', token: 'old-session', expiresAt: '2099-01-01T00:00:00Z' }
const task: ServiceUpdateTask = { id: 'receipt-1', state: 'running', stage: 'installing', current_version: '2.0.0', target_version: '2.0.11', downloaded_bytes: 100, total_bytes: 100, bytes_per_second: 0, started_at: '2026-09-08T00:00:00Z', updated_at: '2026-09-08T00:00:01Z' }
function Background() {
  return <div>{useSession().session ? 'Management background' : 'Login page'}</div>
}
function renderFlow(client: QueryClient) {
  return render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><ServiceUpdateFlow><Background /></ServiceUpdateFlow></SessionProvider></I18nProvider></QueryClientProvider>)
}
afterEach(() => { cleanup(); sessionStorage.clear(); localStorage.clear(); vi.unstubAllGlobals() })

it('retains the management background after a restart rejects the session and shows the actual failure', async () => {
  saveSession(session)
  writeServiceUpdateMarker({ targetVersion: task.target_version, baseURL: session.baseURL, task })
  let current = task
  const fetchMock = vi.fn(async (url: string, init?: RequestInit) => {
    if (url.endsWith('/service/update/task')) {
      expect(new Headers(init?.headers).get('Authorization')).toBe('Bearer receipt-1')
      return Response.json({ task: current })
    }
    return Response.json({ error: { code: 'UNAUTHORIZED', message: 'session expired' } }, { status: 401 })
  })
  vi.stubGlobal('fetch', fetchMock)
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  renderFlow(client)
  await act(async () => { await api(session, '/system').catch(() => {}) })
  expect(sessionStorage.getItem('sempre.session.v1')).toBeNull()
  expect(screen.getByText('Management background')).toBeInTheDocument()
  expect(screen.queryByText('Login page')).not.toBeInTheDocument()
  current = { ...task, state: 'failed', stage: 'failed', error: 'Sempre installer exited with code 1' }
  await act(async () => { await client.invalidateQueries({ queryKey: serviceUpdateTaskKey }) })
  expect(await screen.findByText(current.error!)).toBeInTheDocument()
  fireEvent.click(screen.getAllByRole('button', { name: 'Close' }).at(-1)!)
  expect(screen.getByText('Login page')).toBeInTheDocument()
  client.clear()
})

it('restores an interrupted update without a login session and reads its completion receipt', async () => {
  writeServiceUpdateMarker({ targetVersion: task.target_version, baseURL: session.baseURL, task })
  vi.stubGlobal('fetch', vi.fn(async () => Response.json({ task: { ...task, state: 'succeeded', stage: 'completed' } })))
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  renderFlow(client)
  await waitFor(() => expect(screen.getByRole('button', { name: 'Sign in again' })).toBeInTheDocument())
  expect(screen.queryByText('Login page')).not.toBeInTheDocument()
  client.clear()
})
