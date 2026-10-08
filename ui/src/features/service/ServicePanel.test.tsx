import { ToastProvider } from '@acme/components'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
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
  render(<QueryClientProvider client={client}><ToastProvider><I18nProvider><SessionProvider><ServiceUpdateFlow><ServicePanel /></ServiceUpdateFlow></SessionProvider></I18nProvider></ToastProvider></QueryClientProvider>)

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
