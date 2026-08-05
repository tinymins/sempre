/// <reference types="node" />

import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { Button } from './ui'

const stylesheet = readFileSync(resolve(process.cwd(), 'src/index.css'), 'utf8')

describe('Control typography', () => {
  afterEach(() => cleanup())

  it('keeps the native font reset below utility styles', () => {
    expect(stylesheet).toMatch(/@layer base\s*{\s*button,\s*input,\s*select,\s*textarea\s*{\s*font: inherit;/)

    render(<Button>Apply</Button>)
    expect(screen.getByRole('button', { name: 'Apply' })).toHaveClass('text-sm')
  })
})
