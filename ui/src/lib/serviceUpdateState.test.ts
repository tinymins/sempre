import { afterEach, expect, it, vi } from 'vitest'
import { clearServiceUpdateMarker, readServiceUpdateMarker, writeServiceUpdateMarker } from './serviceUpdateState'

afterEach(() => { clearServiceUpdateMarker(); vi.restoreAllMocks() })

it('keeps the active update only in page memory', async () => {
  const writes = vi.spyOn(Storage.prototype, 'setItem')
  writeServiceUpdateMarker({ targetVersion: '2.0.12' })
  expect(readServiceUpdateMarker()?.targetVersion).toBe('2.0.12')
  expect(writes).not.toHaveBeenCalled()
  vi.resetModules()
  const newPage = await import('./serviceUpdateState')
  expect(newPage.readServiceUpdateMarker()).toBeNull()
})
