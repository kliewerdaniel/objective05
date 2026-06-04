// Vitest setup: extend `expect` with the @testing-library/jest-dom
// matchers (toBeInTheDocument, toHaveTextContent, etc.) and stub the
// `matchMedia` API which some component stylesheets read at mount.
import '@testing-library/jest-dom/vitest'

if (!window.matchMedia) {
  window.matchMedia = (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  })
}
