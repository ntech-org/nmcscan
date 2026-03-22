import { browser } from '$app/environment';
import { env } from '$env/dynamic/public';

const API_BASE = env.PUBLIC_API_URL || (browser ? window.location.origin : 'http://localhost:3000');

export const authState = $state({
    apiKey: browser ? localStorage.getItem('nmcscan_api_key') || '' : '',
    isAuthenticated: false,
});

export function setApiKey(key: string) {
    authState.apiKey = key;
    if (browser) localStorage.setItem('nmcscan_api_key', key);
}

export function clearAuth() {
    authState.apiKey = '';
    authState.isAuthenticated = false;
    if (browser) localStorage.removeItem('nmcscan_api_key');
}

export async function fetchWithAuth(endpoint: string, options: RequestInit = {}) {
    const url = endpoint.startsWith('http') ? endpoint : `${API_BASE}${endpoint}`;
    const headers = new Headers(options.headers);
    if (authState.apiKey) {
        headers.set('X-API-Key', authState.apiKey);
    }
    const res = await fetch(url, { ...options, headers });
    
    if (res.status === 401) {
        authState.isAuthenticated = false;
        throw new Error('Unauthorized: Invalid API Key');
    }
    
    if (!res.ok) {
        const text = await res.text();
        throw new Error(text || `HTTP Error: ${res.status}`);
    }
    return res;
}
