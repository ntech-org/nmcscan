<script lang="ts">
    import { onMount } from 'svelte';
    import { fetchWithAuth, authState } from '$lib/state.svelte';

    interface Asn {
        asn: string;
        org: string;
        category: string;
        country: string | null;
        server_count: number;
    }

    let asns = $state<Asn[]>([]);
    let loading = $state(true);
    let error = $state<string | null>(null);

    async function loadAsns() {
        if (!authState.isAuthenticated) return;
        loading = true;
        try {
            const res = await fetchWithAuth('/api/asns');
            asns = await res.json();
        } catch (e) {
            error = e instanceof Error ? e.message : 'Failed to load ASNs';
        } finally {
            loading = false;
        }
    }

    function getCategoryColor(category: string): string {
        if (category.includes('Hosting')) return 'text-blue-400 bg-blue-400/10 border-blue-500/20';
        if (category.includes('Residential')) return 'text-orange-400 bg-orange-400/10 border-orange-500/20';
        return 'text-gray-400 bg-gray-400/10 border-gray-500/20';
    }

    onMount(() => {
        loadAsns();
    });
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold text-white tracking-tight">Network Topology Map</h1>
        <button onclick={loadAsns} aria-label="Refresh ASNs" class="p-2 bg-gray-800 hover:bg-gray-700 border border-gray-700 rounded-lg text-gray-300 transition-colors shadow-sm">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"></path></svg>
        </button>
    </div>

    {#if error}
        <div class="p-4 bg-red-500/10 border border-red-500/20 text-red-400 rounded-xl">
            {error}
        </div>
    {/if}

    <div class="bg-gray-900 border border-gray-800 rounded-xl overflow-hidden shadow-sm">
        <div class="overflow-x-auto">
            <table class="w-full text-left border-collapse">
                <thead>
                    <tr class="bg-gray-950/50 text-gray-400 text-xs uppercase tracking-wider">
                        <th class="p-4 font-medium">ASN</th>
                        <th class="p-4 font-medium">Organization</th>
                        <th class="p-4 font-medium">Classification</th>
                        <th class="p-4 font-medium text-center">Servers</th>
                        <th class="p-4 font-medium text-center">Country</th>
                    </tr>
                </thead>
                <tbody class="divide-y divide-gray-800/50">
                    {#if loading && asns.length === 0}
                        <tr><td colspan="5" class="p-8 text-center text-gray-500"><svg class="animate-spin h-6 w-6 mx-auto text-blue-500" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg></td></tr>
                    {/if}
                    {#each asns as asn}
                        <tr class="hover:bg-gray-800/20 transition-colors">
                            <td class="p-4 font-mono text-sm text-blue-400">{asn.asn}</td>
                            <td class="p-4 text-sm text-gray-200">{asn.org}</td>
                            <td class="p-4">
                                <span class="inline-flex items-center px-2.5 py-0.5 rounded-full border {getCategoryColor(asn.category)} text-xs font-medium">
                                    {asn.category}
                                </span>
                            </td>
                            <td class="p-4 text-center text-sm text-gray-400">
                                {asn.server_count.toLocaleString()}
                            </td>
                            <td class="p-4 text-center">
                                {#if asn.country}
                                    <img src={`https://flagcdn.com/24x18/${asn.country.toLowerCase()}.png`} alt={asn.country} title={asn.country} class="inline-block rounded shadow-sm opacity-80" />
                                {:else}
                                    <span class="text-gray-500">-</span>
                                {/if}
                            </td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
    </div>
</div>
