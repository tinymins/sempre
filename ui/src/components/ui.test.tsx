/// <reference types="node" />

import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { Button, EmptyState } from './ui'

const stylesheet = readFileSync(resolve(process.cwd(), 'src/index.css'), 'utf8')

describe('Control typography', () => {
  afterEach(() => cleanup())

  it('keeps the native font reset below utility styles', () => {
    expect(stylesheet).toMatch(/@layer base\s*{\s*button,\s*input,\s*select,\s*textarea\s*{\s*font: inherit;/)

    render(<Button>Apply</Button>)
    expect(screen.getByRole('button', { name: 'Apply' })).toHaveClass('text-sm')
  })

  it('renders legacy empty states with the shared Empty visual', () => {
    render(<EmptyState title="No data" detail="Nothing has been returned yet." action={<Button>Retry</Button>} />)

    const title = screen.getByText('No data')
    expect(title.closest('p')?.parentElement?.querySelector('svg')).not.toBeNull()
    expect(screen.getByText('Nothing has been returned yet.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument()
  })
})
