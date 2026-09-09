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

  it('defaults to newest connections first and keeps sortable table headers', async () => {
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
    expect(connectionHosts()).toEqual(['newer.example', 'older.example'])
    expect(screen.getByRole('columnheader', { name: 'Started' })).toHaveAttribute('aria-sort', 'descending')

    fireEvent.click(screen.getByRole('button', { name: 'Started' }))
    expect(connectionHosts()).toEqual(['older.example', 'newer.example'])

    fireEvent.click(screen.getByRole('button', { name: 'Download' }))
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

  it('combines multi-select source and process filters with search and clearing', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({ connections: [
      connection('chrome-a.example', 0, 0, '', '10.0.0.1', 'Chrome'),
      connection('curl-a.example', 0, 0, '', '10.0.0.1', 'curl'),
      connection('chrome-b.example', 0, 0, '', '10.0.0.2', 'Chrome'),
      connection('firefox-b.example', 0, 0, '', '10.0.0.2', 'Firefox'),
      connection('unknown.example', 0, 0, ''),
    ] })))
    renderConnections()
    await screen.findByText('chrome-a.example')

    fireEvent.click(screen.getAllByRole('combobox')[0])
    fireEvent.click(within(await screen.findByRole('listbox')).getByText('10.0.0.1', { exact: true }))
    expect(connectionHosts()).toEqual(['chrome-a.example', 'curl-a.example'])
    fireEvent.click(within(screen.getByRole('listbox')).getByText('10.0.0.2', { exact: true }))
    expect(connectionHosts()).toHaveLength(4)
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'Escape' })

    fireEvent.click(screen.getAllByRole('combobox')[1])
    fireEvent.click(within(await screen.findByRole('listbox')).getByText('Chrome', { exact: true }))
    expect(connectionHosts()).toEqual(['chrome-a.example', 'chrome-b.example'])
    fireEvent.click(within(screen.getByRole('listbox')).getByText('curl', { exact: true }))
    expect(connectionHosts()).toEqual(['chrome-a.example', 'curl-a.example', 'chrome-b.example'])
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'Escape' })

    fireEvent.change(screen.getByPlaceholderText('Search'), { target: { value: 'CHROME-B' } })
    expect(connectionHosts()).toEqual(['chrome-b.example'])
    fireEvent.change(screen.getByPlaceholderText('Search'), { target: { value: '' } })
    for (const select of screen.getAllByRole('combobox')) {
      fireEvent.click(within(select).getAllByRole('button').at(-1)!)
    }
    expect(connectionHosts()).toHaveLength(5)

    fireEvent.click(screen.getAllByRole('combobox')[1])
    fireEvent.click(within(await screen.findByRole('listbox')).getByText('-', { exact: true }))
    expect(connectionHosts()).toEqual(['unknown.example'])
  })

  it('automatically refreshes while retaining filters and sorting new connections', async () => {
    const first = connection('first.example', 0, 0, '2026-09-01T01:00:00Z', '10.0.0.1', 'Chrome')
    const fetchMock = vi.fn().mockResolvedValue(Response.json({ connections: [first] }))
    vi.stubGlobal('fetch', fetchMock)
    renderConnections()
    await screen.findByText('first.example')
    fireEvent.click(screen.getAllByRole('combobox')[1])
    fireEvent.click(within(await screen.findByRole('listbox')).getByText('Chrome', { exact: true }))
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'Escape' })
    fetchMock.mockImplementation(async () => Response.json({ connections: [
      first,
      connection('newest.example', 0, 0, '2026-09-01T03:00:00Z', '10.0.0.1', 'Chrome'),
      connection('hidden.example', 0, 0, '2026-09-01T04:00:00Z', '10.0.0.1', 'curl'),
    ] }))
    await screen.findByText('newest.example', {}, { timeout: 3500 })
    expect(connectionHosts()).toEqual(['newest.example', 'first.example'])
    expect(fetchMock).toHaveBeenCalledTimes(2)
  })
})

function connection(host: string, download: number, upload: number, start: string, source_ip?: string, process?: string) {
  return { id: host, metadata: { host, destination_port: '443', network: 'tcp', source_ip, process }, chains: [], download, upload, start }
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
