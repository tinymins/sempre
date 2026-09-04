import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { restartDuration, type RestartTask } from '../lib/restartTask'
import { RuntimeRestartButton } from './RuntimeRestartButton'

const status = { runtime_state: 'running', pending: false, pending_changes: [], actions: { restart: { allowed: true } } }
const makeTask = (): RestartTask => ({
  id: 'task-1', state: 'running', started_at: new Date(Date.now() - 81000).toISOString(), finished_at: null,
  omitted_logs: 0, config_available: true,
  logs: [
    { sequence: 0, timestamp: '2026-09-03T12:34:56Z', stage: 'begin', message: '' },
    { sequence: 1, timestamp: '2026-09-03T12:34:57Z', stage: 'change', message: '', change: { type: 'configuration', fields: ['nodes', 'dns'] } },
    { sequence: 2, timestamp: '2026-09-03T12:34:58Z', stage: 'compiled', message: '' },
    { sequence: 3, timestamp: '2026-09-03T12:34:59Z', stage: 'stdout', message: 'raw <script>alert(1)</script>\nsecond output line' },
  ],
})

describe('asynchronous restart task', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
  })
  afterEach(() => { cleanup(); vi.unstubAllGlobals(); sessionStorage.clear(); localStorage.clear() })

  it('opens immediately, prevents duplicate submissions, and keeps running after closing', async () => {
    let current: RestartTask | null = null
    let accept!: () => void
    const response = new Promise<void>((resolve) => { accept = resolve })
    let posts = 0
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init: RequestInit = {}) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/runtime/restart')) {
        if (init.method === 'POST') {
          posts++
          await response
          current = makeTask()
          return Response.json({ task: current, status })
        }
        return Response.json({ task: current })
      }
      return Response.json(status)
    }))
    renderButtons()
    await waitFor(() => expect(screen.getByRole('button', { name: 'Restart core' })).toBeEnabled())
    fireEvent.click(screen.getByRole('button', { name: 'Restart core' }))
    const confirm = within(screen.getByRole('dialog', { name: 'Restart the core?' })).getByRole('button', { name: 'Restart core' })
    fireEvent.click(confirm)
    expect(await screen.findByRole('log')).toHaveTextContent('Submitting restart task')
    const spinners = await screen.findAllByRole('button', { name: 'Restarting core · view log' })
    expect(spinners).toHaveLength(2)
    expect(spinners[0]).toBeEnabled()
    fireEvent.click(within(screen.getByRole('dialog', { name: /Restarting core/ })).getByText('Close'))
    fireEvent.click(spinners[0])
    expect(await screen.findByRole('log')).toHaveTextContent('Submitting restart task')
    fireEvent.click(within(screen.getByRole('dialog', { name: /Restarting core/ })).getByText('Close'))
    fireEvent.click(spinners[1])
    expect(await screen.findByRole('log')).toHaveTextContent('Submitting restart task')
    accept()
    await waitFor(() => expect(screen.getByRole('log')).toHaveTextContent('Starting core restart'))
    const dialog = screen.getByRole('dialog', { name: /Restarting core/ })
    expect(dialog).toHaveTextContent(/\(1:2\d\)/)
    fireEvent.click(within(dialog).getByText('Close'))
    expect(posts).toBe(1)
    fireEvent.click(spinners[0])
    expect(await screen.findByRole('log')).toHaveTextContent('Starting core restart')
    fireEvent.click(within(screen.getByRole('dialog', { name: /Restarting core/ })).getByText('Close'))
    fireEvent.click(spinners[1])
    expect(await screen.findByRole('log')).toHaveTextContent('Starting core restart')
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Restart the core?' })).not.toBeInTheDocument())
    expect(posts).toBe(1)
  })

  it('recovers a running task, renders raw lines safely, scrolls, and loads its exact configuration', async () => {
    const task = makeTask()
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL) => {
      const url = new URL(String(input))
      if (url.pathname.endsWith('/restart/config')) {
        expect(url.searchParams.get('id')).toBe(task.id)
        return Response.json({ hash: 'hash', content: '{"this_task":"configuration"}' })
      }
      return Response.json(url.pathname.endsWith('/runtime/restart') ? { task } : { ...status, runtime_state: 'restarting', actions: { restart: { allowed: false } } })
    }))
    renderButtons(client)
    await waitFor(() => expect(screen.getAllByRole('button', { name: 'View restart task' })).toHaveLength(2))
    const spinner = screen.getAllByRole('button', { name: 'Restarting core · view log' })[0]
    expect(spinner).toBeEnabled()
    fireEvent.click(spinner)
    const log = await screen.findByRole('log')
    expect(log).toHaveTextContent('Proxy nodes and DNS configuration')
    expect(log).toHaveTextContent('raw <script>alert(1)</script>')
    expect(log.querySelector('script')).toBeNull()
    expect(log).toHaveTextContent('second output line')
    log.focus()
    expect(fireEvent.keyDown(log, { key: 'a', ctrlKey: true })).toBe(false)
    expect(window.getSelection()?.toString()).toContain('second output line')
    expect(window.getSelection()?.toString()).not.toContain('Restarting core')
    Object.defineProperty(log, 'scrollHeight', { value: 2000, configurable: true })
    client.setQueryData(['runtime', 'restart-task'], { task: { ...task, logs: [...task.logs, { sequence: 4, timestamp: task.started_at, stage: 'health_check', message: 'waiting' }] } })
    await waitFor(() => expect(log.scrollTop).toBe(2000))
    fireEvent.click(within(log).getByRole('button', { name: '[View full configuration]' }))
    const config = await screen.findByLabelText('Full configuration')
    expect(config).toHaveTextContent('{"this_task":"configuration"}')
    config.focus()
    expect(fireEvent.keyDown(config, { key: 'a', metaKey: true })).toBe(false)
    expect(window.getSelection()?.toString()).toBe('{"this_task":"configuration"}')
  })

  it('keeps the completed duration fixed and marks rollback as failure', () => {
    expect(restartDuration('2026-09-03T00:00:00Z', '2026-09-03T00:01:21Z', Date.now())).toBe('1:21')
    expect(restartDuration('2026-09-03T00:01:21Z', null, Date.parse('2026-09-03T00:00:00Z'))).toBe('0:00')
  })
})

function renderButtons(client = new QueryClient({ defaultOptions: { queries: { retry: false } } })) {
  return render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><RuntimeRestartButton /><RuntimeRestartButton panel /></SessionProvider></I18nProvider></QueryClientProvider>)
}
