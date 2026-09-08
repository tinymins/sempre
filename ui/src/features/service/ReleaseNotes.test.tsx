import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { ReleaseNotes, selectLocalizedReleaseNotes } from './ReleaseNotes'

const bilingual = `## 简体中文

### 亮点

- **新增**一键升级，通过 \`sempre.run\` 获取。[查看详情](https://sempre.run)

## English

### Highlights

1. Added one-click upgrades through \`sempre.run\`.
2. Kept beta releases out of the stable channel.`

describe('ReleaseNotes', () => {
  afterEach(cleanup)

  it('selects Chinese notes and renders safe Markdown elements', () => {
    render(<ReleaseNotes releases={[{ version: '2.0.9', published_at: '', notes: bilingual }]} locale="zh-CN" />)

    expect(screen.getByRole('heading', { name: 'v2.0.9' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: '亮点' })).toBeInTheDocument()
    expect(screen.getByText('新增').tagName).toBe('STRONG')
    expect(screen.getByText('sempre.run').tagName).toBe('CODE')
    expect(screen.getByRole('link', { name: '查看详情' })).toHaveAttribute('href', 'https://sempre.run')
    expect(screen.queryByText('Added one-click upgrades', { exact: false })).not.toBeInTheDocument()
  })

  it('selects English notes and renders an ordered list', () => {
    const { container } = render(<ReleaseNotes releases={[{ version: '2.0.9', published_at: '', notes: bilingual }]} locale="en" />)

    expect(screen.getByRole('heading', { name: 'Highlights' })).toBeInTheDocument()
    expect(container.querySelectorAll('ol > li')).toHaveLength(2)
    expect(screen.queryByText('新增', { exact: false })).not.toBeInTheDocument()
  })

  it('renders multiple releases in descending API order', () => {
    render(<ReleaseNotes releases={[
      { version: '2.0.9', published_at: '', notes: '## 简体中文\n\n- 第二版\n\n## English\n\n- Second release' },
      { version: '2.0.8', published_at: '', notes: '## 简体中文\n\n- 第一版\n\n## English\n\n- First release' },
    ]} locale="zh-CN" />)

    expect(screen.getAllByRole('heading').map((heading) => heading.textContent)).toEqual(['v2.0.9', 'v2.0.8'])
    expect(screen.getByText('第二版').compareDocumentPosition(screen.getByText('第一版')) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(screen.queryByText('First release')).not.toBeInTheDocument()
  })

  it('keeps legacy single-language notes intact', () => {
    expect(selectLocalizedReleaseNotes('## Highlights\n\n- Fixed updates.', 'zh-CN')).toBe('## Highlights\n\n- Fixed updates.')
  })
})
