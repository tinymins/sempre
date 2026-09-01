import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { Connections } from './Connections'

describe('Connections', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
    sessionStorage.clear()
  })

  it('treats a null connection list as empty', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({ download_total: 0, upload_total: 0, connections: null })))
    renderConnections()

    expect(await screen.findByRole('heading', { name: 'Connections' })).toBeInTheDocument()
    expect(screen.getByText('0 · ↓ 0 B · ↑ 0 B')).toBeInTheDocument()
    expect(await screen.findByText('No data')).toBeInTheDocument()
  })

  it('sorts connections from sortable table headers without a dropdown', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({
      download_total: 300,
      upload_total: 400,
      connections: [
        connection('older.example', 200, 100, '2026-09-01T01:00:00Z'),
        connection('newer.example', 100, 300, '2026-09-01T02:00:00Z'),
      ],
    })))
    renderConnections()

    expect(await screen.findByText('newer.example')).toBeInTheDocument()
    expect(screen.queryByRole('combobox')).not.toBeInTheDocument()
    expect(connectionHosts()).toEqual(['older.example', 'newer.example'])

    fireEvent.click(screen.getByRole('button', { name: 'Upload' }))
    expect(connectionHosts()).toEqual(['newer.example', 'older.example'])
    expect(screen.getByRole('columnheader', { name: 'Upload' })).toHaveAttribute('aria-sort', 'descending')

    fireEvent.click(screen.getByRole('button', { name: 'Upload' }))
    expect(connectionHosts()).toEqual(['older.example', 'newer.example'])
    expect(screen.getByRole('columnheader', { name: 'Upload' })).toHaveAttribute('aria-sort', 'ascending')

    fireEvent.click(screen.getByRole('button', { name: 'Started' }))
    expect(connectionHosts()).toEqual(['newer.example', 'older.example'])

    fireEvent.click(screen.getByRole('button', { name: 'Host' }))
    expect(screen.getByRole('columnheader', { name: 'Host' })).toHaveAttribute('aria-sort', 'descending')
    fireEvent.click(screen.getByRole('button', { name: 'Host' }))
    expect(connectionHosts()).toEqual(['newer.example', 'older.example'])
    expect(screen.getByRole('columnheader', { name: 'Host' })).toHaveAttribute('aria-sort', 'ascending')
  })
})

function connection(host: string, download: number, upload: number, start: string) {
  return { id: host, metadata: { host, destination_port: '443', network: 'tcp' }, chains: [], download, upload, start }
}

function connectionHosts() {
  return screen.getAllByRole('row').slice(1).map((row) => within(row).getAllByRole('cell')[0].textContent).map((value) => value?.replace('443 · tcp', '') || '')
}

function renderConnections() {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <I18nProvider>
        <SessionProvider>
          <Connections />
        </SessionProvider>
      </I18nProvider>
    </QueryClientProvider>,
  )
}
