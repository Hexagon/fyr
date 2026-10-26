import { describe, expect, it } from 'vitest'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { linkCitations } from './citations.js'

// Mirrors Assistant.vue rendering: citations → marked → DOMPurify.
const render = (text, sources) => DOMPurify.sanitize(marked.parse(linkCitations(text, sources)), { USE_PROFILES: { html: true } })

describe('citation rendering pipeline', () => {
  const sources = [{ n: 1, title: 'Vasa', source: 'wiki.zim', link: '/books?file=wiki.zim&path=Gustav_Vasa' }]

  it('keeps citation anchors through markdown and sanitizing', () => {
    const html = render('- King of Sweden **1523** [1].', sources)
    const doc = new DOMParser().parseFromString(html, 'text/html')
    const anchor = doc.querySelector('a.citation')
    expect(anchor).not.toBeNull()
    expect(anchor.getAttribute('href')).toBe('/books?file=wiki.zim&path=Gustav_Vasa')
    expect(anchor.getAttribute('data-citation')).toBe('1')
    expect(anchor.textContent).toBe('[1]')
    expect(doc.querySelector('strong').textContent).toBe('1523')
  })

  it('cannot inject markup through source titles', () => {
    const evil = [{ n: 1, title: '"><img src=x onerror=alert(1)>', source: 'x.md', link: '/books?file=x.md' }]
    const doc = new DOMParser().parseFromString(render('Fact [1].', evil), 'text/html')
    expect(doc.querySelector('img')).toBeNull()
    const anchor = doc.querySelector('a.citation')
    expect(anchor.getAttribute('title')).toBe('"><img src=x onerror=alert(1)> — x.md')
    expect(anchor.attributes.length).toBe(4)
  })
})
