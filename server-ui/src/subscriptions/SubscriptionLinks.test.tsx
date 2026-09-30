import { ToastProvider } from '@acme/components'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { I18nProvider } from '../i18n/provider'
import { targetSuffix, type Target } from './diagnostic-types'
import { SubscriptionLinks } from './SubscriptionLinks'
import { emptyDraft, type Subscription } from './types'

const debugTarget = vi.fn()
vi.mock('./SubscriptionDebug', () => ({
  SubscriptionDebug: ({ initialTarget }: { initialTarget: Target }) => {
    debugTarget(initialTarget)
    return <div>Debug target: {initialTarget.format}</div>
  },
}))

const subscription: Subscription = {
  ...emptyDraft(), id: 'config-1', userId: 'owner', url: 'public-url', remark: 'Example',
  creator: { id: 'owner', name: 'Owner', email: 'owner@example.com' },
  cachedNodeCount: 0, accessCount: 0, lastAccessAt: null, createdAt: 'created', updatedAt: 'revision-1',
  canEdit: true, canDelete: false, canManageAuthorization: false, assignedCustomNodes: [],
}

const singBoxTargets: Target[] = [11, 12, 13, 14].flatMap((version) =>
  ['openwrt', 'windows', 'macos'].map((platform) => ({
    core: 'sing-box', format: `sing-box${version === 11 ? '' : `-v${version}`}-${platform}`,
    version: `1.${version}`, platform, standalone: false,
  })),
)

beforeEach(() => {
  Object.defineProperty(navigator, 'languages', { configurable: true, value: ['en-US'] })
})
afterEach(() => { cleanup(); debugTarget.mockReset() })

it('uses the full sing-box version and consumer path for every public target', () => {
  for (const target of singBoxTargets) {
    expect(targetSuffix(target.format)).toBe(`sing-box/${target.version}/${target.platform}`)
  }
  expect(targetSuffix('sing-box')).toBe('sing-box/1.11/openwrt')
  for (const version of [12, 13, 14]) {
    expect(targetSuffix(`sing-box-v${version}`)).toBe(`sing-box/1.${version}/openwrt`)
  }
  expect(targetSuffix('clash-meta')).toBe('clash-meta')
  expect(targetSuffix('unknown')).toBeNull()
})

it('labels OpenWrt links and opens debug with the selected public target', () => {
  const selected = singBoxTargets.find((target) => target.format === 'sing-box-v13-openwrt')!
  render(<I18nProvider><ToastProvider><SubscriptionLinks subscription={subscription} targets={singBoxTargets} onClose={vi.fn()} /></ToastProvider></I18nProvider>)

  expect(screen.getByText('Sing-box v1.11 OpenWrt')).toBeInTheDocument()
  expect(screen.getByText(`${window.location.origin}/api/public/proxy/public-url/sing-box/1.13/openwrt`)).toBeInTheDocument()
  fireEvent.click(screen.getByRole('button', { name: /debug saved sing-box v1\.13 openwrt/i }))
  expect(debugTarget).toHaveBeenCalledWith(selected)
  expect(screen.getByText('Debug target: sing-box-v13-openwrt')).toBeInTheDocument()
})
