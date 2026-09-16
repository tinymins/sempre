import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '../lib/i18n'
import { SessionProvider } from '../lib/session'
import { Gateway } from './Gateway'

const gatewayStatus = {
  config: {
    schema: 2,
    topology: 'local-pve',
    lan: { interface: 'vmbr1', gateway_cidr: '10.10.8.1/21', wan_interface: '', nat_enabled: true },
    dhcp: { enabled: false, range_start: '10.10.8.100', range_end: '10.10.8.200', lease_time: '12h', reservations: [] },
    pve: { port: 22, user: 'root', apply_persistent: false },
  },
  runtime: { dhcp_running: false, dhcp_leases: [] },
  inventory: {
    supported: true,
    default_interface: 'vmbr0',
    recommended_lan_interfaces: ['vmbr1'],
    local_prefixes: ['10.10.8.0/21'],
    vpn_prefixes: [],
    occupied_prefixes: ['10.10.8.0/21', '10.23.0.0/21'],
    interfaces: [
      { name: 'vmbr0', index: 2, kind: 'bridge', up: true, default_route: true, addresses: ['10.23.0.200/21'] },
      { name: 'vmbr1', index: 3, kind: 'bridge', up: true, default_route: false, addresses: ['10.10.8.1/21'] },
    ],
  },
  validation_errors: [],
  host_plan_available: true,
}

describe('Gateway page', () => {
  let savedConfig: Record<string, unknown> | null

  beforeEach(() => {
    savedConfig = null
    localStorage.setItem('sempre.locale', 'zh-CN')
    sessionStorage.setItem('sempre.session.v1', JSON.stringify({ baseURL: 'http://sempre.test', token: 'session', expiresAt: '2099-01-01T00:00:00Z' }))
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input)).pathname
      if (path.endsWith('/gateway')) {
        if (init?.method === 'PUT') {
          savedConfig = JSON.parse(String(init.body)) as Record<string, unknown>
          return Response.json({ config: savedConfig, reload_requested: false })
        }
        return Response.json(gatewayStatus)
      }
      if (path.endsWith('/network/settings')) return Response.json({ settings: { schema: 2, revision: 1, mode: 'gateway', gateway_capture_host: false, automatic_switching: false, known_networks: [] }, platform: 'linux', gateway_available: true })
      return Response.json({}, { status: 404 })
    }))
  })

  afterEach(() => {
    cleanup()
    sessionStorage.clear()
    vi.unstubAllGlobals()
  })

  function renderGateway() {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    return render(<QueryClientProvider client={client}><I18nProvider><SessionProvider><Gateway /></SessionProvider></I18nProvider></QueryClientProvider>)
  }

  it('selects the detected default interface for a local PVE host', async () => {
    renderGateway()

    const field = (await screen.findByText('WAN interface')).closest('label')
    expect(field).not.toBeNull()
    await waitFor(() => expect(within(field as HTMLElement).getByRole('combobox')).toHaveTextContent('vmbr0'))

    fireEvent.click(screen.getByRole('button', { name: '保存' }))
    await waitFor(() => expect(savedConfig).not.toBeNull())
    expect(savedConfig).toMatchObject({ lan: { wan_interface: 'vmbr0' } })
  })

  it('keeps the remote PVE interface freely editable', async () => {
    renderGateway()

    const topologyLabel = (await screen.findAllByText('Topology')).find((element) => element.tagName === 'SPAN')
    const topology = topologyLabel?.closest('label')
    expect(topology).not.toBeNull()
    fireEvent.click(within(topology as HTMLElement).getByRole('combobox'))
    fireEvent.click(within(await screen.findByRole('listbox')).getByText('Gateway VM/LXC + PVE SSH/manual'))

    const field = screen.getByText('WAN interface').closest('label')
    const input = within(field as HTMLElement).getByRole('textbox')
    expect(input).toHaveAttribute('placeholder', 'Remote PVE interface name')
    fireEvent.change(input, { target: { value: 'vmbr9' } })
    expect(input).toHaveValue('vmbr9')
  })
})
