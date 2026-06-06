// Vitest setup. Loaded once per test file.
//
// - Wires up `@testing-library/jest-dom` matchers (`toBeInTheDocument`,
//   `toHaveTextContent`, etc.) into the global expect.
// - Provides a stable `window.location` so hooks that read it (e.g.
//   the WebSocket hook builds its URL from the host) get a
//   predictable value across tests.

import '@testing-library/jest-dom/vitest';

if (typeof window !== 'undefined') {
  Object.defineProperty(window, 'location', {
    value: { host: 'localhost:5173', protocol: 'http:' },
    writable: true,
    configurable: true,
  });
}
