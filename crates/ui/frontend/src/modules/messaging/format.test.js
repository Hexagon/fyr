import { describe, expect, it } from 'vitest'
import { createApp, h } from 'vue'
import { charCount, deviceColor, firstLine, groupByDay, linkify, mapsLink } from './format.js'

describe('linkify', () => {
  it('splits text and http(s) links', () => {
    expect(linkify('see https://example.org/a?b=1, ok')).toEqual([
      { type: 'text', text: 'see ' },
      { type: 'link', text: 'https://example.org/a?b=1', href: 'https://example.org/a?b=1' },
      { type: 'text', text: ', ok' }
    ])
  })

  it('handles parentheses and trailing punctuation', () => {
    const segs = linkify('(map: http://fyr.local/maps).')
    expect(segs[1]).toMatchObject({ type: 'link', text: 'http://fyr.local/maps' })
    expect(segs[2].text).toBe(').')
  })

  it('never links dangerous schemes', () => {
    for (const text of ['javascript:alert(1)', 'data:text/html,<script>alert(1)</script>', 'vbscript:x', 'file:///etc/passwd']) {
      expect(linkify(text).every((s) => s.type === 'text')).toBe(true)
    }
  })

  it('keeps markup as plain text segments', () => {
    const segs = linkify('<img src=x onerror=alert(1)> https://a.org/"><script>')
    expect(segs[0]).toEqual({ type: 'text', text: '<img src=x onerror=alert(1)> ' })
    expect(segs[1]).toMatchObject({ type: 'link', href: 'https://a.org/' })
    expect(segs[2].text).toBe('"><script>')
  })

  it('renders inert when interpolated by Vue', () => {
    const text = '<b onclick="x()">hi</b> https://ok.org'
    const el = document.createElement('div')
    createApp({
      render: () => h('p', linkify(text).map((s) => (s.type === 'link' ? h('a', { href: s.href }, s.text) : h('span', s.text))))
    }).mount(el)
    expect(el.querySelector('b')).toBeNull()
    expect(el.textContent).toContain('<b onclick="x()">hi</b>')
    expect(el.querySelector('a').getAttribute('href')).toBe('https://ok.org/')
  })
})

describe('helpers', () => {
  it('groups messages by day in order', () => {
    const day = 86400000
    const base = new Date(2026, 0, 10, 12).getTime()
    const groups = groupByDay([{ created_at: base - day }, { created_at: base }, { created_at: base + 1000 }], base)
    expect(groups.map((g) => g.messages.length)).toEqual([1, 2])
    expect(groups[0].label).toBe('Yesterday')
    expect(groups[1].label).toBe('Today')
  })

  it('builds encoded maps deep links', () => {
    expect(mapsLink({ lat: 59.3, lng: 18.06, label: 'Water & food\nsecond line' })).toBe('/maps?lat=59.3&lng=18.06&zoom=15&label=Water+%26+food')
    expect(mapsLink({ lat: 1, lng: 2, label: '' })).toBe('/maps?lat=1&lng=2&zoom=15')
  })

  it('derives stable colours and counts unicode characters', () => {
    expect(deviceColor('a1b2c3')).toBe(deviceColor('a1b2c3'))
    expect(deviceColor('zz')).toMatch(/^hsl\(/)
    expect(charCount('😀ä')).toBe(2)
    expect(firstLine('x'.repeat(100), 10)).toHaveLength(10)
  })
})
