import { ToastProvider } from '@acme/components'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { I18nProvider } from '../i18n/provider'
import { ServerApiError } from '../server-api'
import { SubscriptionEditor } from './SubscriptionEditor'
import { subscriptionApi } from './api'
import { emptyDraft, type Subscription } from './types'

vi.mock('@acme/components', async importOriginal => ({
  ...await importOriginal<typeof import('@acme/components')>(),
  CodeEditor: ({ value = '', onChange, readOnly }: { value?: string; onChange?: (value: string) => void; readOnly?: boolean }) => <textarea aria-label="JSONC" value={value} readOnly={readOnly} onChange={event => onChange?.(event.target.value)} />,
}))
vi.mock('./api', () => ({ subscriptionApi: { get: vi.fn(), defaults: vi.fn(), users: vi.fn(), update: vi.fn() } }))

const saved: Subscription = {
  ...emptyDraft(), id: 'config-1', userId: 'owner', url: 'public-url', remark: 'original',
  creator: { id: 'owner', name: 'Owner', email: 'owner@example.com' },
  cachedNodeCount: 0, accessCount: 0, lastAccessAt: null, createdAt: 'created', updatedAt: 'revision-1',
  canEdit: true, canDelete: true, canManageAuthorization: false, assignedCustomNodes: [],
}

beforeEach(() => {
  Object.defineProperty(navigator, 'languages', { configurable: true, value: ['en-US'] })
  vi.mocked(subscriptionApi.get).mockResolvedValue(saved)
  vi.mocked(subscriptionApi.defaults).mockResolvedValue({ ruleList: '{}', group: '[]', filter: '[]', customConfig: '[]', dnsConfig: '{"shared":{}}' })
  vi.mocked(subscriptionApi.users).mockResolvedValue([])
})
afterEach(() => { cleanup(); vi.resetAllMocks() })

function show() {
  const onSaved = vi.fn()
  render(<I18nProvider><ToastProvider><SubscriptionEditor open id={saved.id} onClose={vi.fn()} onSaved={onSaved} targets={[]} /></ToastProvider></I18nProvider>)
  return onSaved
}

it('saves the shared form explicitly with the existing revision and preserves authorization ownership', async () => {
  vi.mocked(subscriptionApi.update).mockResolvedValue({ ...saved, remark: 'updated' })
  const onSaved = show()
  fireEvent.change(await screen.findByRole('textbox', { name: 'Remark' }), { target: { value: 'updated' } })
  expect(subscriptionApi.update).not.toHaveBeenCalled()
  fireEvent.click(screen.getByRole('button', { name: 'Save' }))
  await waitFor(() => expect(onSaved).toHaveBeenCalledTimes(1))
  const [id, body, revision] = vi.mocked(subscriptionApi.update).mock.calls[0]
  expect(id).toBe(saved.id)
  expect(revision).toBe('revision-1')
  expect(body.remark).toBe('updated')
  expect(body).not.toHaveProperty('authorizedUserIds')
})

it('retains edited settings on a conflicting revision', async () => {
  vi.mocked(subscriptionApi.update).mockRejectedValue(new ServerApiError('conflict', 409))
  const onSaved = show()
  fireEvent.change(await screen.findByRole('textbox', { name: 'Remark' }), { target: { value: 'keep this' } })
  fireEvent.click(screen.getByRole('button', { name: 'Save' }))
  expect(await screen.findByText(/Someone else changed/)).toBeInTheDocument()
  expect(screen.getByRole('textbox', { name: 'Remark' })).toHaveValue('keep this')
  expect(onSaved).not.toHaveBeenCalled()
})

it('prevents edits and saving for a read-only configuration', async () => {
  vi.mocked(subscriptionApi.get).mockResolvedValue({ ...saved, canEdit: false })
  show()
  expect(await screen.findByRole('textbox', { name: 'Remark' })).toBeDisabled()
  expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled()
  expect(subscriptionApi.update).not.toHaveBeenCalled()
})
