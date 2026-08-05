import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
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
    expect(dialog).toHaveStyle({ width: 'calc(100% - 48px)', height: 'calc(100% - 48px)' })
    fireEvent.click(screen.getByRole('button', { name: 'Start debug' }))

    await waitFor(() => expect(mocks.stream).toHaveBeenCalledOnce())
    expect(await screen.findByText('Request configuration')).toBeInTheDocument()
    expect(screen.getByText('Debug complete')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Run again' })).toBeEnabled()
  })
})
