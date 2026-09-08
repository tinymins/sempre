import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, expect, it, vi } from 'vitest'
import { I18nProvider } from '../../lib/i18n'
import { SessionProvider } from '../../lib/session'
import { ServiceUpdateFlow } from './ServiceUpdateFlow'
import { ServicePanel } from './ServicePanel'

afterEach(() => {
  cleanup()
  sessionStorage.clear()
  localStorage.clear()
  vi.unstubAllGlobals()
})

it.each(['2.0.12', '2.0.13'])('allows an available %s upgrade after a historical successful task', async (latest) => {
  localStorage.setItem('sempre.locale', 'en')
  sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  let task = { id: 'previous', state: 'succeeded', stage: 'completed', current_version: '2.0.0', target_version: '2.0.12', started_at: new Date().toISOString() }
  let starts = 0
  vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = new URL(String(input)).pathname
    if (path.endsWith('/system')) return Response.json({ version: '2.0.0', mode: 'system', service: 'running' })
    if (path.endsWith('/service/update/task')) return Response.json({ task })
    if (path.endsWith('/service/update')) {
      if (init?.method === 'POST') {
        starts += 1
        task = { ...task, id: 'next', state: 'running', stage: 'downloading', target_version: latest }
        return Response.json({ task }, { status: 202 })
      }
      return Response.json({ current_version: '2.0.0', latest_version: latest, update_available: true, release_notes: 'Update available' })
    }
    return Response.json({}, { status: 404 })
  }))
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><ServiceUpdateFlow><ServicePanel /></ServiceUpdateFlow></SessionProvider></I18nProvider></QueryClientProvider>)
  await waitFor(() => expect(client.getQueryData(['service', 'update-task'])).toEqual({ task }))
  fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }))
  const upgrade = await screen.findByRole('button', { name: 'Upgrade now' })
  expect(upgrade).toBeEnabled()
  fireEvent.click(upgrade)
  await waitFor(() => expect(starts).toBe(1))
  expect(await screen.findByRole('heading', { name: /Updating Sempre/ })).toBeInTheDocument()
  fireEvent.click(screen.getAllByRole('button', { name: 'Close' }).at(-1)!)
  fireEvent.click(screen.getAllByRole('button', { name: 'Updating · view progress' })[0])
  expect(starts).toBe(1)
  client.clear()
})
