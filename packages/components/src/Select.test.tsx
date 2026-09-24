import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { Select } from './Select'

describe('Select', () => {
  afterEach(() => cleanup())

  it('constrains long option labels inside the dropdown item', async () => {
    const longLabel = 'vmbr0 · 10.23.0.200/24, 192.168.122.1/24, fe80::7cdb:8bff:fe85:de3f/64'

    render(<Select popupMatchSelectWidth defaultValue="vmbr0" options={[{ value: 'vmbr0', label: longLabel }]} />)

    expect(screen.getByText(longLabel)).toHaveClass('truncate')
    expect(screen.getByText(longLabel)).toHaveAttribute('title', longLabel)

    fireEvent.click(screen.getByRole('combobox'))
    const optionLabel = within(await screen.findByRole('listbox')).getByText(longLabel)

    expect(optionLabel).toHaveClass('min-w-0', 'flex-1', 'truncate')
    expect(optionLabel).toHaveAttribute('title', longLabel)
  })
})
