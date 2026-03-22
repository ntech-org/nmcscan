<script lang="ts">
    import { onMount } from 'svelte';
    import { fetchWithAuth, authState } from '$lib/state.svelte';

    interface ExcludeEntry {
        network: string;
        comment: string | null;
    }

    let exclusions = $state<ExcludeEntry[]>([]);
    let newExcludeNetwork = $state('');
    let newExcludeComment = $state('');
    let excludeSubmitting = $state(false);
    let loading = $state(true);
    let error = $state<string | null>(null);

    async function loadExclusions() {
        if (!authState.isAuthenticated) return;
        loading = true;
        try {
            const res = await fetchWithAuth('/api/exclude');
            exclusions = await res.json();
        } catch (e) {
            error = e instanceof Error ? e.message : 'Failed to load exclusions';
        } finally {
            loading = false;
        }
    }

    async function addExclusion() {
        if (!newExcludeNetwork || !authState.isAuthenticated) return;
        excludeSubmitting = true;
        error = null;
        try {
            await fetchWithAuth('/api/exclude', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    network: newExcludeNetwork,
                    comment: newExcludeComment || null
                })
            });
            newExcludeNetwork = '';
            newExcludeComment = '';
            await loadExclusions();
        } catch (e) {
            error = e instanceof Error ? e.message : 'Failed to add exclusion';
        } finally {
            excludeSubmitting = false;
        }
    }

    onMount(() => {
        loadExclusions();
    });
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold text-white tracking-tight">Network Exclusions</h1>
        <button onclick={loadExclusions} aria-label="Refresh Exclusions" class="p-2 bg-gray-800 hover:bg-gray-700 border border-gray-700 rounded-lg text-gray-300 transition-colors shadow-sm">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"></path></svg>
        </button>
    </div>

    {#if error}
        <div class="p-4 bg-red-500/10 border border-red-500/20 text-red-400 rounded-xl">
            {error}
        </div>
    {/if}

    <div class="bg-gray-900 border border-gray-800 rounded-xl p-6 shadow-sm">
        <p class="text-gray-400 text-sm mb-6">Instantly block IP ranges or specific addresses from being scanned.</p>
        
        <form onsubmit={(e) => { e.preventDefault(); addExclusion(); }} class="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div class="md:col-span-1">
                <label for="network" class="block text-xs font-medium text-gray-500 uppercase tracking-wider mb-1">IP or CIDR</label>
                <input
                    id="network"
                    type="text"
                    placeholder="e.g. 1.2.3.4 or 1.2.3.0/24"
                    bind:value={newExcludeNetwork}
                    class="w-full bg-gray-950 border border-gray-700 rounded-lg px-4 py-2.5 text-sm text-gray-200 focus:border-red-500 focus:ring-1 focus:ring-red-500 outline-none transition-all"
                    required
                />
            </div>
            <div class="md:col-span-1">
                <label for="comment" class="block text-xs font-medium text-gray-500 uppercase tracking-wider mb-1">Reason / Comment</label>
                <input
                    id="comment"
                    type="text"
                    placeholder="Who requested this?"
                    bind:value={newExcludeComment}
                    class="w-full bg-gray-950 border border-gray-700 rounded-lg px-4 py-2.5 text-sm text-gray-200 focus:border-blue-500 focus:ring-1 focus:ring-blue-500 outline-none transition-all"
                />
            </div>
            <div class="flex items-end">
                <button 
                    type="submit"
                    disabled={excludeSubmitting || !newExcludeNetwork}
                    class="w-full bg-red-600/90 hover:bg-red-600 disabled:opacity-50 rounded-lg text-white font-medium py-2.5 transition-all shadow-md flex items-center justify-center gap-2"
                >
                    {#if excludeSubmitting}
                        <svg class="animate-spin h-4 w-4 text-white" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg>
                        Processing
                    {:else}
                        Add Exclusion
                    {/if}
                </button>
            </div>
        </form>
    </div>

    <div class="bg-gray-900 border border-gray-800 rounded-xl overflow-hidden shadow-sm">
        <div class="p-4 border-b border-gray-800 bg-gray-950/30">
            <h3 class="font-semibold text-white">Current Exclusions ({exclusions.length})</h3>
        </div>
        <div class="max-h-[500px] overflow-y-auto">
            <table class="w-full text-left border-collapse">
                <thead>
                    <tr class="bg-gray-950/50 text-gray-400 text-xs uppercase tracking-wider sticky top-0">
                        <th class="p-4 font-medium">Network Range</th>
                        <th class="p-4 font-medium">Comment</th>
                    </tr>
                </thead>
                <tbody class="divide-y divide-gray-800/50">
                    {#if loading && exclusions.length === 0}
                        <tr><td colspan="2" class="p-8 text-center text-gray-500"><svg class="animate-spin h-6 w-6 mx-auto text-blue-500" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg></td></tr>
                    {/if}
                    {#each exclusions.slice().reverse() as entry}
                        <tr class="hover:bg-gray-800/20 transition-colors">
                            <td class="p-4 font-mono text-sm text-red-400">{entry.network}</td>
                            <td class="p-4 text-sm text-gray-400 italic">
                                {entry.comment || '-'}
                            </td>
                        </tr>
                    {/each}
                </tbody>
            </table>
        </div>
    </div>
</div>
