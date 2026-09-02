import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { Traffic } from './Traffic'

vi.mock('../components/RuntimeChart', () => ({ RuntimeChart: () => <div data-testid="runtime-chart" /> }))

describe('Traffic', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
    sessionStorage.clear()
  })

  it('renders history and rotation settings returned by the backend', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/runtime/events')) return new Response('')
      return Response.json({
        settings: { retention_hours: 72, max_bytes: 64 * 1024 * 1024 },
        storage_bytes: 1024,
        totals: [{ label: 'example.com', download: 2048, upload: 1024 }],
      })
    }))
    renderTraffic()

    expect(await screen.findByText('example.com')).toBeInTheDocument()
    expect(screen.getByText('Backend history storage')).toBeInTheDocument()
    expect(screen.getByText('1.0 KiB / 64.0 MiB')).toBeInTheDocument()
    expect(screen.getByText('3 days')).toBeInTheDocument()
    expect(screen.getAllByText('History period')).toHaveLength(2)
    expect(screen.getByText('Rolling window')).toBeInTheDocument()
    expect(screen.getByText('Statistics window length')).toBeInTheDocument()
    expect(screen.getByText('History retention time')).toBeInTheDocument()
  })

  it('renders a monthly traffic reset day', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      if (String(input).includes('/runtime/events')) return new Response('')
      return Response.json({
        settings: { retention_hours: 24, reset_day: 21, max_bytes: null },
        storage_bytes: 80,
        totals: [],
      })
    }))
    renderTraffic()

    expect(await screen.findByText('Monthly billing cycle')).toBeInTheDocument()
    expect(screen.getByText('Day 21')).toBeInTheDocument()
    expect(screen.getByText('Retained billing cycles')).toBeInTheDocument()
    expect(screen.getByText('12 cycles')).toBeInTheDocument()
    expect(screen.getByText('80 B / Unlimited')).toBeInTheDocument()
    expect(screen.getByText('Total history storage limit')).toBeInTheDocument()
    expect(screen.getByText(/The statistics window only controls the current summary/)).toBeInTheDocument()
    expect(screen.getByText('Retention time and the total size limit may both be unlimited; the history file will keep growing when they are.')).toBeInTheDocument()
  })

  it('allows an unlimited storage cap when time retention is unlimited', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      if (String(input).includes('/runtime/events')) return new Response('')
      return Response.json({
        settings: { window_hours: 24, retention_hours: null, reset_day: null, retention_months: 12, max_bytes: 32 * 1024 * 1024 },
        storage_bytes: 80,
        totals: [],
      })
    }))
    renderTraffic()

    const field = (await screen.findByText('Total history storage limit')).closest('label')
    expect(field).not.toBeNull()
    fireEvent.click(within(field as HTMLLabelElement).getByRole('combobox'))
    const unlimited = within(await screen.findByRole('listbox')).getByText('Unlimited').parentElement

    expect(unlimited).not.toBeNull()
    expect(unlimited).not.toHaveClass('opacity-50')
  })

  it('sorts traffic totals from every data header', async () => {
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      if (String(input).includes('/runtime/events')) return new Response('')
      return Response.json({
        settings: { retention_hours: 24, max_bytes: 1024 },
        storage_bytes: 0,
        totals: [
          { label: 'zulu.example', download: 3000, upload: 100 },
          { label: 'alpha.example', download: 1000, upload: 300 },
          { label: 'mike.example', download: 2000, upload: 200 },
        ],
      })
    }))
    renderTraffic()
    expect(await screen.findByText('zulu.example')).toBeInTheDocument()

    const header = screen.getByRole('columnheader', { name: 'Download' })
    fireEvent.click(header)

    expect(trafficLabels()).toEqual(['alpha.example', 'mike.example', 'zulu.example'])
    expect(header).toHaveAttribute('aria-sort', 'ascending')
    expect(screen.getByRole('columnheader', { name: 'Host' })).toHaveAttribute('aria-sort', 'none')
    expect(screen.getByRole('columnheader', { name: 'Upload' })).toHaveAttribute('aria-sort', 'none')
    expect(screen.getByRole('columnheader', { name: 'Total traffic' })).toHaveAttribute('aria-sort', 'none')
  })

  it('queries preset and custom history ranges from the right-aligned range control', async () => {
    const now = new Date('2026-09-02T12:00:00').getTime()
    vi.spyOn(Date, 'now').mockReturnValue(now)
    const requests: string[] = []
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.includes('/runtime/events')) return new Response('')
      requests.push(url)
      return Response.json({ settings: {}, storage_bytes: 0, totals: [] })
    }))
    renderTraffic()

    const period = await screen.findByRole('button', { name: 'History period' })
    expect(new URL(requests[0]).searchParams.has('since')).toBe(false)
    fireEvent.click(period)
    fireEvent.click(await screen.findByRole('button', { name: 'Last 6 hours' }))

    await waitFor(() => expect(requests.some((request) => new URL(request).searchParams.get('since') === String(now - 6 * 60 * 60 * 1000))).toBe(true))
    fireEvent.click(screen.getByRole('button', { name: 'Last 6 hours', expanded: false }))
    fireEvent.click(await screen.findByRole('button', { name: 'Custom range' }))
    fireEvent.change(screen.getByLabelText('Start time'), { target: { value: '2026-09-01T08:15' } })
    fireEvent.change(screen.getByLabelText('End time'), { target: { value: '2026-09-02T10:45' } })
    fireEvent.click(screen.getByRole('button', { name: 'Apply' }))

    const since = new Date('2026-09-01T08:15').getTime()
    const until = new Date('2026-09-02T10:45').getTime()
    await waitFor(() => expect(requests.some((request) => {
      const params = new URL(request).searchParams
      return params.get('since') === String(since) && params.get('until') === String(until)
    })).toBe(true))
    expect(screen.getByRole('button', { name: 'Custom range', expanded: false })).toBeInTheDocument()
  })
})

function trafficLabels() {
  return screen.getAllByRole('row').slice(1).map((row) => within(row).getAllByRole('cell')[0].textContent)
}

function renderTraffic() {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <I18nProvider>
        <SessionProvider>
          <Traffic />
        </SessionProvider>
      </I18nProvider>
    </QueryClientProvider>,
  )
}
