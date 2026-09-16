import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, expect, it, vi } from 'vitest'
import { I18nProvider } from '../../lib/i18n'
import { SessionProvider } from '../../lib/session'
import { ServiceUpdateFlow } from './ServiceUpdateFlow'
import { clearServiceUpdateMarker } from '../../lib/serviceUpdateState'
import { ServicePanel } from './ServicePanel'

afterEach(() => {
  cleanup()
  clearServiceUpdateMarker()
  sessionStorage.clear()
  localStorage.clear()
  vi.unstubAllGlobals()
})

it.each(['2.0.12', '2.0.13'])('allows an available %s upgrade after a historical successful task', async (latest) => {
  localStorage.setItem('sempre.locale', 'en')
  sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  let task = { id: 'previous', state: 'succeeded', stage: 'completed', current_version: '2.0.0', target_version: '2.0.12', started_at: new Date().toISOString() }
  let starts = 0
  let accept!: () => void
  const accepted = new Promise<void>((resolve) => { accept = resolve })
  vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = new URL(String(input)).pathname
    if (path.endsWith('/system')) return Response.json({ version: '2.0.0', mode: 'system', service: 'running' })
    if (path.endsWith('/service/update/task')) return Response.json({ task })
    if (path.endsWith('/service/update/settings')) return Response.json({ settings: { schema: 1, allow_prerelease: false } })
    if (path.endsWith('/service/update')) {
      if (init?.method === 'POST') {
        starts += 1
        await accepted
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
  expect(screen.queryByRole('heading', { name: /Sempre update complete/ })).not.toBeInTheDocument()
  expect(screen.getByText('Creating update task')).toBeInTheDocument()
  await act(async () => { accept() })
  expect(await screen.findByRole('heading', { name: /Updating Sempre/ })).toBeInTheDocument()
  fireEvent.click(screen.getAllByRole('button', { name: 'Close' }).at(-1)!)
  fireEvent.click(screen.getAllByRole('button', { name: 'Updating · view progress' })[0])
  expect(starts).toBe(1)
  client.clear()
})

it('requires confirmation before enabling preview updates and saves the choice', async () => {
  localStorage.setItem('sempre.locale', 'en')
  sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  let allowPrerelease = false
  let writes = 0
  vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = new URL(String(input)).pathname
    if (path.endsWith('/system')) return Response.json({ version: '2.0.0', mode: 'system', service: 'running' })
    if (path.endsWith('/service/update/task')) return Response.json({ task: null })
    if (path.endsWith('/service/update/settings')) {
      if (init?.method === 'PUT') {
        writes += 1
        allowPrerelease = Boolean(JSON.parse(String(init.body)).allow_prerelease)
      }
      return Response.json({ settings: { schema: 1, allow_prerelease: allowPrerelease } })
    }
    return Response.json({}, { status: 404 })
  }))
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><ServiceUpdateFlow><ServicePanel /></ServiceUpdateFlow></SessionProvider></I18nProvider></QueryClientProvider>)

  const preview = await screen.findByRole('switch', { name: 'Allow preview updates' })
  const checkForUpdates = screen.getByRole('button', { name: 'Check for updates' })
  await waitFor(() => expect(preview).toBeEnabled())
  expect(preview.compareDocumentPosition(checkForUpdates) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  expect(preview).toHaveAttribute('aria-checked', 'false')
  fireEvent.click(preview)
  expect(writes).toBe(0)
  const dialog = await screen.findByRole('dialog', { name: 'Allow preview updates?' })
  expect(within(dialog).getByText(/may be unstable/)).toBeInTheDocument()
  fireEvent.click(within(dialog).getByRole('button', { name: 'Allow preview updates' }))

  await waitFor(() => expect(writes).toBe(1))
  expect(preview).toHaveAttribute('aria-checked', 'true')
  fireEvent.click(preview)
  await waitFor(() => expect(writes).toBe(2))
  expect(preview).toHaveAttribute('aria-checked', 'false')
  client.clear()
})

it('keeps one-click update and reports an invalid uploaded package in the existing flow', async () => {
  localStorage.setItem('sempre.locale', 'en')
  sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  let task: Record<string, unknown> | null = null
  let autoStarts = 0
  let uploadedName = ''
  vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = new URL(String(input))
    const path = url.pathname
    if (path.endsWith('/system')) return Response.json({ version: '2.0.0', mode: 'system', service: 'running' })
    if (path.endsWith('/service/update/task')) return Response.json({ task })
    if (path.endsWith('/service/update/settings')) return Response.json({ settings: { schema: 1, allow_prerelease: false } })
    if (path.endsWith('/service/update/upload')) {
      uploadedName = url.searchParams.get('name') || ''
      expect(init?.body).toBeInstanceOf(File)
      task = { id: 'upload', state: 'failed', stage: 'validating', current_version: '2.0.0', target_version: '', downloaded_bytes: 10, total_bytes: 10, bytes_per_second: 0, started_at: new Date().toISOString(), updated_at: new Date().toISOString(), error: 'uploaded update package must contain .sempre and install.sh at its root' }
      return Response.json({ task }, { status: 202 })
    }
    if (path.endsWith('/service/update')) {
      if (init?.method === 'POST') autoStarts += 1
      return Response.json({ current_version: '2.0.0', latest_version: '2.1.0', update_available: true, release_notes: 'Update available' })
    }
    return Response.json({}, { status: 404 })
  }))
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><ServiceUpdateFlow><ServicePanel /></ServiceUpdateFlow></SessionProvider></I18nProvider></QueryClientProvider>)

  await waitFor(() => expect(screen.getByRole('button', { name: 'Upload update package' })).toBeEnabled())
  fireEvent.click(screen.getByRole('button', { name: 'Check for updates' }))
  expect(await screen.findByRole('button', { name: 'Upgrade now' })).toBeEnabled()
  fireEvent.change(screen.getByLabelText('Upload update package'), { target: { files: [new File(['archive'], 'offline-update.zip', { type: 'application/zip' })] } })

  await waitFor(() => expect(uploadedName).toBe('offline-update.zip'))
  expect(autoStarts).toBe(0)
  expect(await screen.findByText('uploaded update package must contain .sempre and install.sh at its root')).toBeInTheDocument()
  expect(screen.getByRole('heading', { name: /Sempre update failed/ })).toBeInTheDocument()
  client.clear()
})
