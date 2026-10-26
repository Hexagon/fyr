import { describe, expect, it } from 'vitest'
import { createMessagingStorage } from './storage.js'

const fakeStorage = () => {
  const map = new Map()
  return {
    map,
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k)
  }
}

const throwingStorage = {
  getItem: () => { throw new Error('SecurityError') },
  setItem: () => { throw new Error('QuotaExceededError') },
  removeItem: () => { throw new Error('SecurityError') }
}

const channel = { name: 'camp', channelId: 'a'.repeat(32), authKey: 'b'.repeat(64), encKey: 'c'.repeat(64) }

describe('messaging storage', () => {
  it('persists a stable device id, short tag and nickname', () => {
    const backing = fakeStorage()
    const a = createMessagingStorage(backing)
    const id = a.deviceId()
    expect(id).toMatch(/^[0-9a-f]{32}$/)
    expect(a.deviceTag()).toBe(id.slice(0, 6))
    a.setNickname('  Ada Lovelace  ')
    const b = createMessagingStorage(backing)
    expect(b.deviceId()).toBe(id)
    expect(b.nickname()).toBe('Ada Lovelace')
  })

  it('remembers and forgets channels without ever storing a passphrase', () => {
    const backing = fakeStorage()
    const s = createMessagingStorage(backing)
    s.rememberChannel({ ...channel, passphrase: 'hunter2' })
    s.rememberChannel(channel) // de-duplicated
    expect(s.channels()).toHaveLength(1)
    expect([...backing.map.values()].join('')).not.toContain('hunter2')
    s.setLastSeen(channel.channelId, 5)
    s.setLastSeen(channel.channelId, 3)
    expect(s.lastSeen(channel.channelId)).toBe(5)
    s.forgetChannel(channel.channelId)
    expect(s.channels()).toEqual([])
    expect(s.lastSeen(channel.channelId)).toBe(0)
    expect(() => s.rememberChannel({ ...channel, authKey: 'nope' })).toThrow()
  })

  it('ignores corrupted data', () => {
    const backing = fakeStorage()
    backing.setItem('fyr.messages.channels', '{not json')
    backing.setItem('fyr.messages.dismissedAlerts', '"x"')
    const s = createMessagingStorage(backing)
    expect(s.channels()).toEqual([])
    expect(s.dismissedAlerts().size).toBe(0)
    backing.setItem('fyr.messages.channels', JSON.stringify([{ name: 'x', channelId: 'zz' }, channel]))
    expect(s.channels()).toHaveLength(1)
  })

  it('keeps working in memory when storage throws', () => {
    const s = createMessagingStorage(throwingStorage)
    s.setNickname('Bob')
    expect(s.nickname()).toBe('Bob')
    s.rememberChannel(channel)
    expect(s.channels()).toHaveLength(1)
    s.dismissAlert(7)
    expect(s.dismissedAlerts().has(7)).toBe(true)
    expect(s.deviceId()).toBe(s.deviceId())
  })

  it('works with no storage at all', () => {
    const s = createMessagingStorage(null)
    s.setNickname('Eve')
    expect(s.nickname()).toBe('Eve')
  })
})
