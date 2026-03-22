<script lang="ts">
    import { onMount } from 'svelte';
    import { fetchWithAuth, authState } from '$lib/state.svelte';
    
    interface Stats {
        total_servers: number;
        online_servers: number;
        total_players: number;
        asn_hosting: number;
        asn_residential: number;
        asn_unknown: number;
    }

    let stats = $state<Stats | null>(null);
    let testScanning = $state(false);
    let testScanResult = $state<{ status: string; servers_added: number } | null>(null);
    let testScanRegion = $state('quick');
    let error = $state<string | null>(null);

    let refreshInterval: ReturnType<typeof setInterval>;

    async function loadData() {
        if (!authState.isAuthenticated) return;
        try {
            const res = await fetchWithAuth('/api/stats');
            stats = await res.json();
        } catch (e) {
            console.error("Data load error:", e);
        }
    }

    async function triggerTestScan() {
        testScanning = true;
        testScanResult = null;
        try {
            const payload: any = {};
            if (testScanRegion === 'quick') payload.quick = true;
            else if (testScanRegion !== 'default') payload.region = testScanRegion;

            const res = await fetchWithAuth('/api/scan/test', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(payload)
            });
            testScanResult = await res.json();
            setTimeout(() => loadData(), 2000);
        } catch (e) {
            error = e instanceof Error ? e.message : 'Failed to trigger test scan';
        } finally {
            testScanning = false;
        }
    }

    onMount(() => {
        loadData();
        refreshInterval = setInterval(() => {
            loadData();
        }, 30000);

        return () => clearInterval(refreshInterval);
    });
</script>

<div class="space-y-8">
    <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold text-white tracking-tight">System Overview</h1>
    </div>

    <!-- Stats Grid -->
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm relative overflow-hidden group">
            <div class="absolute top-0 right-0 p-4 opacity-10 group-hover:opacity-20 transition-opacity">
                <svg class="w-12 h-12 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01"></path></svg>
            </div>
            <div class="text-gray-400 font-medium text-sm">Discovered Servers</div>
            <div class="text-4xl font-bold text-white mt-2 tracking-tight">{stats?.total_servers?.toLocaleString() ?? 0}</div>
        </div>
        <div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm relative overflow-hidden group">
            <div class="absolute top-0 right-0 p-4 opacity-10 group-hover:opacity-20 transition-opacity">
                <svg class="w-12 h-12 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5.121 17.804A13.937 13.937 0 0112 16c2.5 0 4.847.655 6.879 1.804M15 10a3 3 0 11-6 0 3 3 0 016 0zm6 2a9 9 0 11-18 0 9 9 0 0118 0z"></path></svg>
            </div>
            <div class="text-gray-400 font-medium text-sm">Online Right Now</div>
            <div class="text-4xl font-bold text-green-400 mt-2 tracking-tight">{stats?.online_servers?.toLocaleString() ?? 0}</div>
        </div>
        <div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm relative overflow-hidden group">
            <div class="absolute top-0 right-0 p-4 opacity-10 group-hover:opacity-20 transition-opacity">
                <svg class="w-12 h-12 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"></path></svg>
            </div>
            <div class="text-gray-400 font-medium text-sm">Players Online</div>
            <div class="text-4xl font-bold text-purple-400 mt-2 tracking-tight">{stats?.total_players?.toLocaleString() ?? 0}</div>
        </div>
        <div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm relative overflow-hidden group">
            <div class="absolute top-0 right-0 p-4 opacity-10 group-hover:opacity-20 transition-opacity">
                <svg class="w-12 h-12 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"></path></svg>
            </div>
            <div class="text-gray-400 font-medium text-sm">Hosting ASNs Tracked</div>
            <div class="text-4xl font-bold text-orange-400 mt-2 tracking-tight">{stats?.asn_hosting?.toLocaleString() ?? 0}</div>
        </div>
    </div>

    <!-- Test Scan Panel -->
    <div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm">
        <div class="flex items-center justify-between flex-wrap gap-4">
            <div>
                <h2 class="text-lg font-semibold text-white">Manual Scanner Override</h2>
                <p class="text-gray-400 text-sm mt-1">Force an immediate scan on specific network segments.</p>
            </div>
            <div class="flex items-center gap-3">
                <select
                    class="bg-gray-950 border border-gray-700 rounded-lg px-4 py-2 text-sm text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none"
                    bind:value={testScanRegion}
                >
                    <option value="quick">Quick Test (10 servers)</option>
                    <option value="default">All Known Servers (50)</option>
                    <option value="us">US Servers</option>
                    <option value="eu">EU Servers</option>
                </select>
                <button
                    class="px-5 py-2 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 rounded-lg text-sm font-medium transition-all shadow-md active:scale-95 flex items-center gap-2"
                    onclick={triggerTestScan}
                    disabled={testScanning}
                >
                    {#if testScanning}
                        <svg class="animate-spin h-4 w-4 text-white" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg>
                        Deploying...
                    {:else}
                        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"></path></svg>
                        Execute Scan
                    {/if}
                </button>
            </div>
        </div>
        {#if error}
            <div class="mt-4 p-3 bg-red-500/10 border border-red-500/20 text-red-400 rounded-lg text-sm">{error}</div>
        {/if}
        {#if testScanResult}
            <div class="mt-4 p-3 bg-green-500/10 border border-green-500/20 text-green-400 rounded-lg text-sm flex items-center gap-2">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path></svg>
                Successfully dispatched tasks for {testScanResult.servers_added} servers.
            </div>
        {/if}
    </div>
</div>
