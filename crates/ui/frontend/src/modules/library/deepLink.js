// Deep-link contract for the Books page (used by Home, global search, the
// assistant and anything else that wants to open a book):
//
//   /books?file=<filename>               open that book
//   /books?file=<f>&path=<article path>  open that article inside a ZIM
//   /books?file=<f>&q=<query>            open the book and run in-book search
//
// `path` and `q` are only meaningful together with `file`. These helpers are
// pure so the URL <-> reader-state mapping can be unit tested.

export const BOOKS_ROUTE_PATH = '/books'

const MAX_FILE = 1024
const MAX_PATH = 4096
const MAX_QUERY = 256
// eslint-disable-next-line no-control-regex
const CONTROL_CHARS = /[\u0000-\u001f\u007f]/

const firstString = (value) => {
  if (Array.isArray(value)) {
    const found = value.find((item) => typeof item === 'string')
    return found === undefined ? null : found
  }
  return typeof value === 'string' ? value : null
}

const cleanFile = (value) => {
  const file = firstString(value)
  if (!file || file.length > MAX_FILE) return null
  if (CONTROL_CHARS.test(file) || file.includes('/') || file.includes('\\')) return null
  if (file === '.' || file === '..') return null
  return file
}

const cleanPath = (value) => {
  const path = firstString(value)
  if (!path) return null
  const stripped = path.replace(/^\/+/, '')
  if (!stripped || stripped.length > MAX_PATH || CONTROL_CHARS.test(stripped)) return null
  return stripped
}

const cleanQuery = (value) => {
  const q = firstString(value)
  if (!q) return null
  const trimmed = q.replace(/\s+/g, ' ').trim()
  if (!trimmed) return null
  return trimmed.length > MAX_QUERY ? trimmed.slice(0, MAX_QUERY) : trimmed
}

/**
 * Parse a vue-router `route.query` object into reader state.
 * Returns `{ file, path, q }` where every field is a string or null; when
 * `file` is null the others are null too.
 */
export const parseReaderQuery = (query) => {
  const source = query && typeof query === 'object' ? query : {}
  const file = cleanFile(source.file)
  if (!file) return { file: null, path: null, q: null }
  return {
    file,
    path: cleanPath(source.path),
    q: cleanQuery(source.q)
  }
}

/** Build a vue-router query object for the given reader state. */
export const buildReaderQuery = (state) => {
  const parsed = parseReaderQuery(state)
  if (!parsed.file) return {}
  const query = { file: parsed.file }
  if (parsed.path) query.path = parsed.path
  if (parsed.q) query.q = parsed.q
  return query
}

export const readerStateEquals = (a, b) => {
  const left = parseReaderQuery(a)
  const right = parseReaderQuery(b)
  return left.file === right.file && left.path === right.path && left.q === right.q
}

/** Route location object, e.g. for `router.push(bookRoute('x.zim', { path }))`. */
export const bookRoute = (file, { path = null, q = null } = {}) => ({
  path: BOOKS_ROUTE_PATH,
  query: buildReaderQuery({ file, path, q })
})

/** Plain href string, e.g. for `<a :href>` or `<router-link :to>`. */
export const buildBookDeepLink = (file, options = {}) => {
  const query = bookRoute(file, options).query
  const params = new URLSearchParams()
  for (const key of ['file', 'path', 'q']) {
    if (query[key]) params.set(key, query[key])
  }
  const search = params.toString()
  return search ? `${BOOKS_ROUTE_PATH}?${search}` : BOOKS_ROUTE_PATH
}
