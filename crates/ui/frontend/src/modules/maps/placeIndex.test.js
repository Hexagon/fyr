import { describe, expect, it } from 'vitest'
import { setLocale } from '../../i18n/index.js'
import { describePlaceEntry, isMapFilename, normalizePlaceStatus, shouldPollPlaces } from './placeIndex.js'

const payload = {
  enabled: true,
  auto_max_mb: 4096,
  min_free_mb: 1024,
  low_disk: false,
  maps: [
    { filename: 'sweden.pmtiles', state: 'ready', content: 'vector', places: 12345, processed_tiles: 10, total_tiles: 10, percent: 100, auto: true, index_bytes: 2048 },
    { filename: 'norway.mbtiles', state: 'indexing', content: 'vector', places: 7, percent: 41.6, auto: true },
    { filename: 'aerial.mbtiles', state: 'unsupported', content: 'raster', note: 'Raster map: there are no place names to index' },
    { filename: 'odd.pmtiles', state: 'weird', content: 'banana', percent: 'x', places: -5 },
    { filename: 'big.pmtiles', state: 'not_indexed', content: 'vector', auto: false },
    { nope: true },
    null
  ]
}

describe('place index status', () => {
  it('recognises map filenames', () => {
    expect(isMapFilename('a.PMTiles')).toBe(true)
    expect(isMapFilename('b.mbtiles')).toBe(true)
    expect(isMapFilename('c.zim')).toBe(false)
    expect(isMapFilename(null)).toBe(false)
  })

  it('normalises the payload defensively', () => {
    const status = normalizePlaceStatus(payload)
    expect(status.enabled).toBe(true)
    expect([...status.byFile.keys()]).toEqual(['sweden.pmtiles', 'norway.mbtiles', 'aerial.mbtiles', 'odd.pmtiles', 'big.pmtiles'])
    expect(status.byFile.get('odd.pmtiles')).toMatchObject({ state: 'not_indexed', content: 'unknown', percent: 0, places: 0 })
    expect(normalizePlaceStatus(null)).toMatchObject({ enabled: true, lowDisk: false })
  })

  it('describes each state with the right actions', () => {
    const { byFile } = normalizePlaceStatus(payload)
    const ready = describePlaceEntry(byFile.get('sweden.pmtiles'))
    expect(ready).toMatchObject({ label: 'Places ready', tone: 'ok', canDelete: true, canBuild: false })
    expect(ready.title).toBe('12,345 places indexed')
    const busy = describePlaceEntry(byFile.get('norway.mbtiles'))
    expect(busy).toMatchObject({ label: 'Indexing 42%', showProgress: true, canDelete: true })
    const raster = describePlaceEntry(byFile.get('aerial.mbtiles'))
    expect(raster).toMatchObject({ label: 'Raster map', canBuild: false, canDelete: false })
    expect(describePlaceEntry(byFile.get('big.pmtiles'))).toMatchObject({ label: 'Not indexed', canBuild: true })
    expect(describePlaceEntry(undefined)).toMatchObject({ label: 'Not indexed', canBuild: true })
    expect(describePlaceEntry(byFile.get('sweden.pmtiles'), { enabled: false })).toMatchObject({ label: 'Off', canBuild: false, canDelete: false })
  })

  it('polls only while work is pending', () => {
    expect(shouldPollPlaces(normalizePlaceStatus(payload))).toBe(true)
    expect(shouldPollPlaces(normalizePlaceStatus({ maps: [payload.maps[0], payload.maps[2]] }))).toBe(false)
    expect(shouldPollPlaces(normalizePlaceStatus({ maps: [{ filename: 'r.mbtiles', state: 'not_indexed', content: 'raster', auto: true }] }))).toBe(false)
    expect(shouldPollPlaces(null)).toBe(false)
  })

  it('speaks Swedish', async () => {
    try {
      await setLocale('sv', { persist: false })
      const { byFile } = normalizePlaceStatus(payload)
      expect(describePlaceEntry(byFile.get('sweden.pmtiles')).label).toBe('Platser klara')
      expect(describePlaceEntry(byFile.get('aerial.mbtiles')).label).toBe('Rasterkarta')
    } finally {
      await setLocale('en', { persist: false })
    }
  })
})
