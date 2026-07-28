# Implementation Plan

[Overview]
Add a public Tools page to the Fyr offline platform providing unit conversion utilities and encryption/ciphering tools, suitable for offline/off-grid use with zero server dependencies.

The Tools page will be a new Vue 3 single-file component (`Tools.vue`) added to the frontend SPA. All tool logic runs entirely client-side in the browser—no server API endpoints are needed. This aligns with the offline-first architecture and avoids adding mutating endpoints that would require admin middleware. The page follows the existing dark-themed UI patterns established by Home, Settings, and Assistant pages, using the same CSS variable system and card-based layout. The tools are organized into two tabbed sections: (1) Unit Converters covering length, mass, temperature, area, volume, speed, and data storage, and (2) Encryption & Ciphers covering AES encrypt/decrypt, Base64 encode/decode, ROT13, and SHA-256/MD5 hashing. All ciphering uses the browser's native Web Crypto API and standard library functions—no external cryptographic libraries are needed.

[Types]
No new TypeScript or Rust type definitions are required; all conversions operate on primitive JavaScript types within the Vue component.

The component's internal state uses reactive refs with the following logical shape:
- `activeTab` (string): `'converters'` | `'ciphers'` — controls which tab panel is visible
- Converter state: per-category refs for input value, from-unit, to-unit, and computed result. Each of the seven converter categories (length, mass, temperature, area, volume, speed, data) stores its own independent state as a local reactive object.
- Cipher state: mode selector (encrypt/decrypt), algorithm selector (AES-CBC/Base64/ROT13/hash), text input, optional key/password input, and computed output.

[Files]
One new file, two existing files modified.

**New files:**
- `crates/ui/frontend/src/pages/Tools.vue` — Complete Vue SFC with template, script, and scoped styles implementing the dual-tab Tools page with all converter and cipher logic.

**Existing files modified:**
- `crates/ui/frontend/src/main.js` — Import Tools.vue, register `/tools` route with name `'Tools'` and meta `{ title: 'Tools', subtitle: 'Unit converters, encryption, and ciphering utilities', headerLabel: 'Offline utilities' }`, no `requiresAdmin` flag. Also add a `<router-link>` for Tools in the navbar.
- `crates/ui/frontend/src/App.vue` — Add a `<router-link to="/tools">` nav item in the `.navbar-menu` between the Assistant and Settings entries.

No files are deleted or moved. The server crate (`crates/server`) requires zero changes since all functionality is client-side.

**Documentation files modified:**
- `docs/user/USER_MANUAL.md` — Add Section 9: Tools page describing the two tabs (Unit Converters and Encryption & Ciphers), listing available converters, cipher algorithms, and noting all operations are local/offline-safe.
- `docs/developer/DEVELOPER_MANUAL.md` — Add a brief entry in Section 1 or a new sub-section noting the Tools page as a purely client-side feature with no server dependencies.
- `README.md` — Add "Tools" to the main pages list in the feature overview section.

[Functions]
No functions are added to external service files or server handlers. All logic lives within the Tools.vue component as local functions.

**New functions (all in `crates/ui/frontend/src/pages/Tools.vue`):**
- `convertLength(value, fromUnit, toUnit)` → number — converts between mm, cm, m, km, in, ft, yd, mi
- `convertMass(value, fromUnit, toUnit)` → number — converts between mg, g, kg, oz, lb
- `convertTemperature(value, fromUnit, toUnit)` → number — converts between C, F, K
- `convertArea(value, fromUnit, toUnit)` → number — converts between mm², cm², m², km², ha, in², ft², ac
- `convertVolume(value, fromUnit, toUnit)` → number — converts between mL, L, m³, fl_oz, gal, cup
- `convertSpeed(value, fromUnit, toUnit)` → number — converts between m/s, km/h, mph, knot
- `convertData(value, fromUnit, toUnit)` → number — converts between B, KB, MB, GB, TB, KiB, MiB, GiB
- `aesEncrypt(plaintext, password)` → Promise\<string\> — Web Crypto AES-CBC encrypt with PBKDF2 key derivation, returns hex
- `aesDecrypt(ciphertextHex, password)` → Promise\<string\> — Web Crypto AES-CBC decrypt from hex
- `base64Encode(text)` → string — btoa wrapper
- `base64Decode(text)` → string — atob wrapper
- `rot13(text)` → string — character-level ROT13 rotation
- `sha256(text)` → Promise\<string\> — Web Crypto SHA-256 digest as hex
- `md5(text)` → string — pure JS MD5 implementation (no Web Crypto native MD5 exists; use a compact inline implementation)

**No existing functions are modified or removed.**

[Classes]
No classes are added, modified, or removed. The codebase does not use class-based components; Vue 3 Composition API with `<script setup>` is the established pattern, which Tools.vue will follow.

[Dependencies]
No new npm packages, Cargo crates, or external libraries are required.

- The browser-native `crypto.subtle` API provides AES-CBC encryption/decryption, PBKDF2 key derivation, and SHA-256 hashing.
- `btoa`/`atob` provide Base64 encoding/decoding.
- All unit conversions use pure arithmetic—no external math library needed.
- MD5 is implemented as a compact inline function (~50 lines of pure JS), avoiding any dependency.
- The `marked` and `DOMPurify` packages already exist in the frontend dependency tree (used by Assistant.vue) and are not needed by the Tools page.

[Testing]
No server-side test changes are needed. The Tools page is a purely client-side visual feature.

- Manual testing: navigate to `/tools`, verify both tabs render, test each converter with known values, test each cipher tool with known plaintext/ciphertext pairs.
- Existing project tests (`cargo test --workspace --all-targets`) should continue to pass since no Rust code is changed.
- Frontend build (`npm run build` in `crates/ui/frontend`) should succeed without errors.
- After build, verify the SPA bundle in `public/static/` includes the new Tools page chunk.

[Implementation Order]
Implement in phases: create the Vue component first, then integrate into routing/navigation, then update documentation.

1. Create `crates/ui/frontend/src/pages/Tools.vue` with the full component implementation — tabbed layout, all seven converter categories, four cipher tools, dark-themed styling matching existing pages
2. Modify `crates/ui/frontend/src/main.js` — add Tools import, route definition (public, no `requiresAdmin`), and add the nav link for Tools in App.vue
3. Modify `crates/ui/frontend/src/App.vue` — add `<router-link to="/tools">Tools</router-link>` in `.navbar-menu`
4. Update `docs/user/USER_MANUAL.md` — add Section 9 documenting the Tools page features and usage
5. Update `docs/developer/DEVELOPER_MANUAL.md` — add brief note about Tools page as client-side feature
6. Update `README.md` — add Tools to the main pages list
7. Build and verify — run `npm run build` in `crates/ui/frontend` to confirm no errors, then run `cargo check -p server` to confirm no regressions