// In-reader back/forward history for ZIM articles. Pure data structure; the
// Books page owns one instance per open archive and resets it on book change.

export const createArticleHistory = (limit = 100) => {
  let entries = []
  let index = -1

  const current = () => (index >= 0 ? entries[index] : null)

  /** Record a newly visited entry (drops any forward history). */
  const visit = (entry) => {
    if (!entry) return
    if (current() === entry) return
    entries = entries.slice(0, index + 1)
    entries.push(entry)
    if (entries.length > limit) {
      entries = entries.slice(entries.length - limit)
    }
    index = entries.length - 1
  }

  const canBack = () => index > 0
  const canForward = () => index >= 0 && index < entries.length - 1

  /** Move back; returns the entry to load, or null. */
  const back = () => {
    if (!canBack()) return null
    index -= 1
    return entries[index]
  }

  const forward = () => {
    if (!canForward()) return null
    index += 1
    return entries[index]
  }

  /** Undo a back()/forward() whose load failed, so the pointer matches what is shown. */
  const restore = (previousIndex) => {
    if (previousIndex >= -1 && previousIndex < entries.length) index = previousIndex
  }

  const reset = () => {
    entries = []
    index = -1
  }

  return {
    visit,
    back,
    forward,
    canBack,
    canForward,
    current,
    restore,
    reset,
    get index() {
      return index
    },
    get size() {
      return entries.length
    }
  }
}
