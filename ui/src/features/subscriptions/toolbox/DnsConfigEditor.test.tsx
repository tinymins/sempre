import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { I18nProvider } from '@/lib/i18n'
import DnsConfigEditor from './DnsConfigEditor'

vi.mock('@monaco-editor/react', () => ({
  default: ({ value = '', onChange }: { value?: string; onChange?: (value: string) => void }) => (
    <textarea aria-label="JSONC editor" value={value} onChange={(event) => onChange?.(event.target.value)} />
  ),
  loader: { config: vi.fn() },
}))
vi.mock('monaco-editor', () => ({}))

describe('DnsConfigEditor', () => {
  afterEach(() => cleanup())

  it('persists Linux system DNS takeover settings', () => {
    localStorage.setItem('sempre.locale', 'en')
    const onChange = vi.fn()

    render(
      <I18nProvider>
        <DnsConfigEditor
          features={['dns.local_upstream', 'dns.system_takeover']}
          onChange={onChange}
        />
      </I18nProvider>,
    )

    expect(screen.getByText('System DNS')).toBeInTheDocument()
    expect(screen.getByText('Take over system DNS')).toBeInTheDocument()

    fireEvent.click(screen.getByRole('switch'))

    expect(onChange).toHaveBeenCalledWith(JSON.stringify({
      shared: { systemDnsTakeoverEnabled: true },
    }, null, 2))
  })
})
