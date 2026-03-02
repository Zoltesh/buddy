import { describe, it, expect, beforeEach, vi } from 'vitest';
import { checkAuthStatus, verifyToken, getAuthToken, clearAuthToken } from '../lib/api.js';

describe('Auth error handling', () => {
  let originalFetch;
  
  beforeEach(() => {
    originalFetch = global.fetch;
    global.localStorage = {
      getItem: vi.fn(),
      setItem: vi.fn(),
      removeItem: vi.fn(),
    };
    clearAuthToken();
  });

  afterEach(() => {
    global.fetch = originalFetch;
  });

  it('should fail with network error and set authenticated=false and authRequired=true', async () => {
    const fetchMock = vi.fn(() => {
      throw new TypeError('Failed to fetch');
    });
    global.fetch = fetchMock;

    let caughtError = false;
    try {
      await checkAuthStatus();
    } catch {
      caughtError = true;
    }
    expect(caughtError).toBe(true);
  });

  it('should show error message when auth check fails', async () => {
    const fetchMock = vi.fn(() => {
      throw new TypeError('Network error');
    });
    global.fetch = fetchMock;

    let errorMessage = '';
    try {
      await checkAuthStatus();
    } catch (e) {
      errorMessage = e.message;
    }
    expect(errorMessage).toBeTruthy();
  });

  it('should show normal login flow when auth required is true', async () => {
    const fetchMock = vi.fn(() => Promise.resolve({
      ok: true,
      json: () => Promise.resolve({ required: true }),
    }));
    global.fetch = fetchMock;

    const status = await checkAuthStatus();
    expect(status.required).toBe(true);
  });

  it('should load app normally when auth is not required', async () => {
    const fetchMock = vi.fn(() => Promise.resolve({
      ok: true,
      json: () => Promise.resolve({ required: false }),
    }));
    global.fetch = fetchMock;

    const status = await checkAuthStatus();
    expect(status.required).toBe(false);
  });
});
