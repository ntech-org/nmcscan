<script lang="ts">
    import { fetchWithAuth, authState } from '$lib/state.svelte';
    import { goto } from '$app/navigation';
    import MinecraftText from '$lib/components/MinecraftText.svelte';
    import * as Table from '$lib/components/ui/table';
    import { Button } from '$lib/components/ui/button';
    import { Input } from '$lib/components/ui/input';

    interface PlayerResponse {
        ip: string;
        port: number;
        player_name: string;
        last_seen: string;
    }

    let playerSearchQuery = $state('');
    let playerSearchResults = $state<PlayerResponse[]>([]);
    let playerSearchLoading = $state(false);
    let error = $state<string | null>(null);

    async function searchPlayers() {
        if (!authState.isAuthenticated || playerSearchQuery.length < 3) return;
        playerSearchLoading = true;
        error = null;
        try {
            const res = await fetchWithAuth(`/api/players?name=${encodeURIComponent(playerSearchQuery)}`);
            playerSearchResults = await res.json();
        } catch (e) {
            error = e instanceof Error ? e.message : 'Player search failed';
        } finally {
            playerSearchLoading = false;
        }
    }

    function formatLastSeen(dateStr: string | null): string {
        if (!dateStr) return 'Never';
        return new Date(dateStr).toLocaleString();
    }
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <h1 class="text-2xl font-bold tracking-tight">Global Player Search</h1>
    </div>

    <p class="text-muted-foreground text-sm">Track player sightings across all scanned networks.</p>

    <div class="flex gap-3">
        <div class="relative flex-1">
            <svg class="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z"></path></svg>
            <Input
                type="text"
                placeholder="Enter player name (min 3 chars)..."
                bind:value={playerSearchQuery}
                onkeydown={(e: KeyboardEvent) => e.key === 'Enter' && searchPlayers()}
                class="pl-10 h-10"
            />
        </div>
        <Button
            onclick={searchPlayers}
            disabled={playerSearchLoading || playerSearchQuery.length < 3}
            class="h-10 px-6"
        >
            {#if playerSearchLoading}
                <svg class="animate-spin h-4 w-4" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path></svg>
                Searching
            {:else}
                Search
            {/if}
        </Button>
    </div>
    {#if error}
        <div class="p-3 bg-destructive/10 border border-destructive/20 text-destructive rounded-lg text-sm">{error}</div>
    {/if}

    {#if playerSearchResults.length > 0}
        <Table.Root>
            <Table.Header>
                <Table.Row>
                    <Table.Head>Player Name</Table.Head>
                    <Table.Head>Server IP</Table.Head>
                    <Table.Head>Last Seen</Table.Head>
                    <Table.Head class="text-right">Actions</Table.Head>
                </Table.Row>
            </Table.Header>
            <Table.Body>
                {#each playerSearchResults as player}
                    <Table.Row class="cursor-pointer" onclick={() => goto(`/explore/servers/${player.ip}:${player.port}`)}>
                        <Table.Cell>
                            <div class="flex items-center gap-3">
                                <img src={`https://minotar.net/helm/${player.player_name}/32.png`} alt={player.player_name} class="w-7 h-7 rounded" onerror={(e) => { (e.currentTarget as HTMLImageElement).src = 'data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24"><rect width="24" height="24" fill="%23333"/><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z" fill="%23666"/></svg>'; }} />
                                <span class="truncate max-w-[200px] font-medium">
                                    <MinecraftText text={player.player_name} />
                                </span>
                            </div>
                        </Table.Cell>
                        <Table.Cell class="font-mono text-sm text-primary">{player.ip}</Table.Cell>
                        <Table.Cell class="text-sm text-muted-foreground">{formatLastSeen(player.last_seen)}</Table.Cell>
                        <Table.Cell class="text-right">
                            <Button variant="outline" size="sm">View Server</Button>
                        </Table.Cell>
                    </Table.Row>
                {/each}
            </Table.Body>
        </Table.Root>
    {:else if playerSearchQuery.length >= 3 && !playerSearchLoading && playerSearchResults.length === 0}
        <div class="py-16 text-center text-muted-foreground">
            <svg class="w-12 h-12 mx-auto mb-3 opacity-20" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"></path></svg>
            <p class="text-sm">No records found for player "{playerSearchQuery}"</p>
        </div>
    {/if}
</div>
