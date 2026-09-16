import { afterEach, describe, expect, it, vi } from 'vitest'
import { randomUuid } from './randomUuid'

describe('randomUuid', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('uses the native implementation when available', () => {
    const native = vi.fn(() => '11111111-2222-4333-8444-555555555555')
    vi.stubGlobal('crypto', { randomUUID: native })

    expect(randomUuid()).toBe('11111111-2222-4333-8444-555555555555')
    expect(native).toHaveBeenCalledOnce()
  })

  it('creates an RFC 4122 version 4 UUID when randomUUID is unavailable', () => {
    vi.stubGlobal('crypto', { getRandomValues: (bytes: Uint8Array) => bytes.fill(0) })

    expect(randomUuid()).toBe('00000000-0000-4000-8000-000000000000')
  })
})
