import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useState } from 'react'
import { AcmeContentBoundary } from '@/components/AcmeContentBoundary'
import { I18nProvider } from '@/lib/i18n'
import SourceDebugModal from './SourceDebugModal'

const mocks = vi.hoisted(() => ({ stream: vi.fn() }))

vi.mock('@/generated/rust-api', () => ({
  proxyApi: { debugSource: { stream: mocks.stream } },
}))

const item = {
  enabled: true,
  url: 'https://example.com/subscription',
  prefix: '',
  remark: 'Example',
  cacheTtlMinutes: 60,
  fetchUa: '',
  fetchMode: 'auto' as const,
}

function SourceDebugHarness() {
  const [open, setOpen] = useState(true)
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>Open debug modal</button>
      <SourceDebugModal open={open} item={item} onClose={() => setOpen(false)} />
    </>
  )
}

describe('SourceDebugModal', () => {
  beforeEach(() => {
    localStorage.setItem('sempre.locale', 'en')
    Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, value: vi.fn() })
    mocks.stream.mockReset()
  })

  afterEach(() => cleanup())

  it('uses the Modal size preset and renders streamed debug steps', async () => {
    mocks.stream.mockImplementation(async (_input, onStep) => {
      onStep({
        type: 'config',
        data: {
          url: item.url,
          ua: 'clash.meta',
          prefix: '',
          cacheTtlMinutes: 60,
          mode: 'bypass-cache',
          fetchMode: 'auto',
          proxyEndpoint: null,
          maxAttempts: 3,
          timeoutMs: 10000,
        },
      })
      onStep({
        type: 'done',
        data: { success: true, resultSource: 'live', nodeCount: 2, totalDurationMs: 120 },
      })
    })

    render(
      <I18nProvider>
        <AcmeContentBoundary>
          <SourceDebugModal open item={item} onClose={() => undefined} />
        </AcmeContentBoundary>
      </I18nProvider>,
    )

    const dialog = await screen.findByRole('dialog', { name: /Subscription Source Debug/ })
    expect(dialog).toHaveStyle({ width: '100%', height: '100%', maxWidth: '100%', borderRadius: 0 })
    fireEvent.click(screen.getByRole('button', { name: 'Start debug' }))

    await waitFor(() => expect(mocks.stream).toHaveBeenCalledOnce())
    expect(await screen.findByText('Request configuration')).toBeInTheDocument()
    expect(screen.getByText('Debug complete')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Run again' })).toBeEnabled()
  })

  it('preserves debug content through the exit animation and resets on reopen', async () => {
    mocks.stream.mockImplementation(async (_input, onStep) => {
      onStep({
        type: 'done',
        data: { success: true, resultSource: 'live', nodeCount: 2, totalDurationMs: 120 },
      })
    })

    render(
      <I18nProvider>
        <AcmeContentBoundary>
          <SourceDebugHarness />
        </AcmeContentBoundary>
      </I18nProvider>,
    )

    fireEvent.click(await screen.findByRole('button', { name: 'Start debug' }))
    expect(await screen.findByText('Debug complete')).toBeInTheDocument()

    const closeButtons = screen.getAllByRole('button', { name: 'Close' })
    fireEvent.click(closeButtons[closeButtons.length - 1])

    expect(screen.getByText('Debug complete')).toBeInTheDocument()
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: 'Open debug modal' }))
    expect(await screen.findByRole('button', { name: 'Start debug' })).toBeEnabled()
    expect(screen.queryByText('Debug complete')).not.toBeInTheDocument()
  })
})
