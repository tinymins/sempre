import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { List } from './List'

describe('List', () => {
  afterEach(() => cleanup())

  it('uses the shared Empty visual when no rows are available', () => {
    const { container } = render(<List dataSource={[]} />)

    expect(screen.getByText('暂无数据')).toBeInTheDocument()
    expect(container.querySelector('svg')).not.toBeNull()
  })
})
