// Vitest setup: make every English message namespace available, as the
// route loaders would in the app, so module tests see real English text.
import { NAMESPACES, loadNamespace } from './index.js'

await Promise.all(NAMESPACES.map((ns) => loadNamespace(ns)))
