<script lang="ts">
    import { onMount } from 'svelte';
    import { page } from '$app/stores';
    import { authState, setApiKey, clearAuth, fetchWithAuth } from '$lib/state.svelte';
    
    let { children } = $props();

    let loading = $state(false);
    let error = $state<string | null>(null);
    let inputKey = $state(authState.apiKey);

    const navItems = [
        { path: '/admin', label: 'Overview', icon: 'M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6' },
        { path: '/admin/servers', label: 'Servers', icon: 'M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01' },
        { path: '/admin/players', label: 'Players', icon: 'M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z' },
        { path: '/admin/asns', label: 'Network & ASN', icon: 'M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10' },
        { path: '/admin/exclusions', label: 'Exclusions', icon: 'M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z' }
    ];

    async function login() {
        try {
            error = null;
            loading = true;
            setApiKey(inputKey);
            
            // Test auth
            await fetchWithAuth('/api/stats');
            authState.isAuthenticated = true;
        } catch (e) {
            error = e instanceof Error ? e.message : 'Login failed';
            clearAuth();
        } finally {
            loading = false;
        }
    }

    onMount(() => {
        if (authState.apiKey) {
            login();
        }
    });

</script>

<svelte:head>
    <title>NMCScan Dashboard</title>
</svelte:head>

<div class="min-h-screen bg-[#0b0f19] text-gray-200 font-sans selection:bg-blue-500/30 flex">
    {#if !authState.isAuthenticated}
        <div class="flex items-center justify-center min-h-screen w-full bg-[#0b0f19] bg-[radial-gradient(ellipse_at_center,_var(--tw-gradient-stops))] from-blue-900/20 via-[#0b0f19] to-[#0b0f19]">
            <div class="bg-gray-900/80 backdrop-blur-xl border border-gray-800 p-8 rounded-2xl shadow-2xl w-full max-w-md">
                <div class="text-center mb-8">
                    <div class="inline-flex items-center justify-center w-16 h-16 rounded-full bg-blue-500/10 text-blue-400 mb-4 shadow-[0_0_30px_rgba(59,130,246,0.3)]">
                        <svg class="w-8 h-8" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"></path></svg>
                    </div>
                    <h1 class="text-3xl font-bold bg-clip-text text-transparent bg-gradient-to-r from-blue-400 to-cyan-300">NMCScan</h1>
                    <p class="text-gray-400 mt-2">Secure access required</p>
                </div>
                
                <form onsubmit={(e) => { e.preventDefault(); login(); }} class="space-y-4">
                    <div>
                        <label for="apiKey" class="block text-sm font-medium text-gray-400 mb-1">API Key</label>
                        <input
                            type="password"
                            id="apiKey"
                            bind:value={inputKey}
                            class="w-full bg-gray-950/50 border border-gray-700 rounded-lg px-4 py-3 text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none transition-all"
                            placeholder="Enter your secure key"
                            required
                        />
                    </div>
                    {#if error}
                        <div class="text-red-400 text-sm p-3 bg-red-400/10 rounded-lg border border-red-400/20">{error}</div>
                    {/if}
                    <button
                        type="submit"
                        disabled={loading}
                        class="w-full bg-blue-600 hover:bg-blue-500 disabled:bg-blue-800 disabled:opacity-50 text-white font-medium py-3 rounded-lg transition-all shadow-lg hover:shadow-blue-500/25 active:scale-[0.98]"
                    >
                        {loading ? 'Authenticating...' : 'Access Dashboard'}
                    </button>
                </form>
            </div>
        </div>
    {:else}
        <!-- Sidebar -->
        <aside class="w-64 bg-gray-900/50 backdrop-blur-md border-r border-gray-800 flex flex-col fixed h-full z-40">
            <div class="p-6 border-b border-gray-800 flex items-center gap-3">
                <div class="w-8 h-8 rounded bg-gradient-to-br from-blue-500 to-cyan-400 flex items-center justify-center shadow-lg shadow-blue-500/30 shrink-0">
                    <svg class="w-5 h-5 text-white" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"></path></svg>
                </div>
                <h1 class="text-xl font-bold tracking-tight text-white truncate">NMCScan</h1>
            </div>
            
            <nav class="flex-1 p-4 space-y-1.5 overflow-y-auto">
                <div class="text-xs font-semibold text-gray-500 uppercase tracking-wider mb-4 px-3 mt-4">Menu</div>
                {#each navItems as item}
                    <a
                        href={item.path}
                        class="flex items-center gap-3 px-3 py-2.5 rounded-lg transition-all text-sm font-medium {$page.url.pathname === item.path || ($page.url.pathname.startsWith('/admin/servers') && item.path === '/admin/servers') ? 'bg-blue-500/10 text-blue-400' : 'text-gray-400 hover:text-gray-200 hover:bg-gray-800/50'}"
                    >
                        <svg class="w-5 h-5 {$page.url.pathname === item.path || ($page.url.pathname.startsWith('/admin/servers') && item.path === '/admin/servers') ? 'text-blue-400' : 'text-gray-500'}" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d={item.icon}></path></svg>
                        {item.label}
                    </a>
                {/each}
            </nav>

            <div class="p-4 border-t border-gray-800">
                <div class="flex items-center gap-3 px-3 py-2 mb-2">
                    <span class="relative flex h-2.5 w-2.5">
                      <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                      <span class="relative inline-flex rounded-full h-2.5 w-2.5 bg-green-500"></span>
                    </span>
                    <span class="text-sm font-medium text-gray-300">Scanner Online</span>
                </div>
                <button onclick={clearAuth} class="w-full flex items-center justify-center gap-2 text-gray-400 hover:text-white transition-colors text-sm font-medium px-4 py-2.5 rounded-lg bg-gray-800/50 hover:bg-gray-800 border border-gray-700">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1"></path></svg>
                    Disconnect
                </button>
            </div>
        </aside>
        
        <!-- Main Content Area -->
        <main class="flex-1 ml-64 min-w-0">
            <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8 animate-in fade-in duration-500">
                {@render children()}
            </div>
        </main>
    {/if}
</div>
