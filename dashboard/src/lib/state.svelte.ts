import { browser } from '$app/environment';

export const authState = $state({
    isAuthenticated: true, // We assume true if the user can reach these routes (guarded by layout)
});

export async function fetchWithAuth(endpoint: string, options: RequestInit = {}) {
    // Relative endpoints already go through SvelteKit proxy
    const res = await fetch(endpoint, options);
    
    if (res.status === 401) {
        throw new Error('Unauthorized: Session expired or invalid');
    }
    
    if (!res.ok) {
        const text = await res.text();
        throw new Error(text || `HTTP Error: ${res.status}`);
    }
    return res;
}
