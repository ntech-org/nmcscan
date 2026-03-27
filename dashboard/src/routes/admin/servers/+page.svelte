<script lang="ts">
    import { onMount, untrack } from "svelte";
    import { fetchWithAuth } from "$lib/state.svelte";
    import { goto } from "$app/navigation";
    import { page } from "$app/state";
    import MinecraftText from "$lib/components/MinecraftText.svelte";

    import * as Card from "$lib/components/ui/card";
    import * as Table from "$lib/components/ui/table";
    import { Button } from "$lib/components/ui/button";
    import { Badge } from "$lib/components/ui/badge";
    import { Input } from "$lib/components/ui/input";
    import { Separator } from "$lib/components/ui/sidebar"; // Wait, Separator is in ui
    import { Separator as UISeparator } from "$lib/components/ui/separator";

    import Search from "@lucide/svelte/icons/search";
    import RefreshCcw from "@lucide/svelte/icons/refresh-ccw";
    import MoreHorizontal from "@lucide/svelte/icons/more-horizontal";
    import ExternalLink from "@lucide/svelte/icons/external-link";
    import Filter from "@lucide/svelte/icons/filter";
    import ChevronDown from "@lucide/svelte/icons/chevron-down";
    import Zap from "@lucide/svelte/icons/zap";
    import Globe2 from "@lucide/svelte/icons/globe-2";
    import HardDrive from "@lucide/svelte/icons/hard-drive";
    import Monitor from "@lucide/svelte/icons/monitor";

    interface Server {
        ip: string;
        port: number;
        server_type: string;
        status: string;
        players_online: number;
        players_max: number;
        motd: string | null;
        version: string | null;
        priority: number;
        last_seen: string | null;
        consecutive_failures: number;
        asn?: string | null;
        country?: string | null;
        favicon?: string | null;
        brand?: string | null;
    }

    let servers = $state<Server[]>([]);
    let loading = $state(true);
    let loadingMore = $state(false);
    let hasMore = $state(true);
    let error = $state<string | null>(null);

    // Filters
    let searchQuery = $state(page.url.searchParams.get("search") || "");
    let statusFilter = $state(page.url.searchParams.get("status") || "all");
    let serverTypeFilter = $state(
        page.url.searchParams.get("server_type") || "all",
    );
    let brandFilter = $state(page.url.searchParams.get("brand") || "all");
    let asnCategory = $state(
        page.url.searchParams.get("asn_category") || "all",
    );
    let sortBy = $state(page.url.searchParams.get("sort_by") || "players");
    let sortOrder = $state(page.url.searchParams.get("sort_order") || "desc");

    let searchTimeout: ReturnType<typeof setTimeout>;

    async function searchServers(append = false) {
        if (append) loadingMore = true;
        else {
            loading = true;
            servers = [];
        }

        error = null;
        try {
            const params = new URLSearchParams();
            params.set("limit", "50");
            if (searchQuery) params.set("search", searchQuery);
            if (statusFilter !== "all") params.set("status", statusFilter);
            if (brandFilter !== "all") params.set("brand", brandFilter);
            if (serverTypeFilter !== "all")
                params.set("server_type", serverTypeFilter);
            if (asnCategory !== "all") params.set("asn_category", asnCategory);
            params.set("sort_by", sortBy);
            params.set("sort_order", sortOrder);

            if (!append) {
                goto(`?${params.toString()}`, {
                    replaceState: true,
                    noScroll: true,
                    keepFocus: true,
                });
            }

            if (append && servers.length > 0) {
                const last = servers[servers.length - 1];
                params.set("cursor_ip", last.ip);
                if (sortBy === "players")
                    params.set(
                        "cursor_players",
                        last.players_online.toString(),
                    );
                if (sortBy === "last_seen" && last.last_seen)
                    params.set("cursor_last_seen", last.last_seen);
            }

            const res = await fetchWithAuth(
                `/api/servers?${params.toString()}`,
            );
            const newServers: Server[] = await res.json();

            if (append) {
                servers = [...servers, ...newServers];
            } else {
                servers = newServers;
            }

            hasMore = newServers.length === 50;
        } catch (e) {
            error = e instanceof Error ? e.message : "Search failed";
        } finally {
            loading = false;
            loadingMore = false;
        }
    }

    function onFilterChange() {
        clearTimeout(searchTimeout);
        searchTimeout = setTimeout(() => {
            untrack(() => searchServers());
        }, 500);
    }

    onMount(() => {
        searchServers();
    });
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <div class="space-y-1">
            <h2
                class="text-3xl font-bold tracking-tight italic flex items-center gap-3"
            >
                <HardDrive class="text-primary" />
                Server Directory
            </h2>
            <p class="text-muted-foreground italic text-sm">
                Monitor all discovered Minecraft servers across the IPv4 space.
            </p>
        </div>
        <Button
            variant="outline"
            size="icon"
            onclick={() => searchServers()}
            disabled={loading}
            class="rounded-full h-10 w-10 shadow-sm"
        >
            <RefreshCcw class="h-4 w-4 {loading ? 'animate-spin' : ''}" />
        </Button>
    </div>

    <Card.Root class="bg-card shadow-lg border-muted">
        <Card.Header class="p-6 pb-4">
            <div class="flex flex-col md:flex-row gap-4">
                <div class="relative flex-1">
                    <Search
                        class="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground"
                    />
                    <Input
                        placeholder="Search by IP, MOTD, or Version..."
                        class="pl-10 h-10 italic"
                        bind:value={searchQuery}
                        oninput={onFilterChange}
                    />
                </div>
                <div class="flex flex-wrap gap-2">
                    <select
                        bind:value={statusFilter}
                        onchange={onFilterChange}
                        class="h-10 px-3 py-2 bg-background border rounded-md text-xs font-medium focus:ring-1 focus:ring-primary outline-none transition-all cursor-pointer"
                    >
                        <option value="all">All Status</option>
                        <option value="online">Online Only</option>
                        <option value="offline">Offline Only</option>
                    </select>
                    <select
                        bind:value={serverTypeFilter}
                        onchange={onFilterChange}
                        class="h-10 px-3 py-2 bg-background border rounded-md text-xs font-medium focus:ring-1 focus:ring-primary outline-none transition-all cursor-pointer"
                    >
                        <option value="all">All Types</option>
                        <option value="java">Java (Standard)</option>
                        <option value="bedrock">Bedrock (MCPE)</option>
                    </select>
                    <select
                        bind:value={sortBy}
                        onchange={onFilterChange}
                        class="h-10 px-3 py-2 bg-background border rounded-md text-xs font-medium focus:ring-1 focus:ring-primary outline-none transition-all cursor-pointer"
                    >
                        <option value="players">Sort: Players</option>
                        <option value="last_seen">Sort: Last Seen</option>
                        <option value="ip">Sort: IP Address</option>
                    </select>
                </div>
            </div>
        </Card.Header>
        <Card.Content class="p-0">
            <Table.Root>
                <Table.Header>
                    <Table.Row
                        class="bg-muted/30 hover:bg-muted/30 uppercase tracking-widest text-[10px] font-bold"
                    >
                        <Table.Head class="w-16"></Table.Head>
                        <Table.Head>Server Address</Table.Head>
                        <Table.Head>Status</Table.Head>
                        <Table.Head>Players</Table.Head>
                        <Table.Head>Software</Table.Head>
                        <Table.Head class="text-right">Action</Table.Head>
                    </Table.Row>
                </Table.Header>
                <Table.Body>
                    {#if loading && servers.length === 0}
                        <Table.Row>
                            <Table.Cell
                                colspan={6}
                                class="h-24 text-center italic text-muted-foreground"
                            >
                                Retrieving data from scanner...
                            </Table.Cell>
                        </Table.Row>
                    {:else if servers.length === 0}
                        <Table.Row>
                            <Table.Cell
                                colspan={6}
                                class="h-24 text-center italic text-muted-foreground"
                            >
                                No servers found matching current filters.
                            </Table.Cell>
                        </Table.Row>
                    {/if}

                    {#each servers as server}
                        <Table.Row
                            class="group cursor-pointer"
                            onclick={() =>
                                goto(
                                    `/admin/servers/${server.ip}:${server.port}`,
                                )}
                        >
                            <Table.Cell>
                                {#if server.favicon}
                                    <img
                                        src={server.favicon}
                                        alt=""
                                        class="w-8 h-8 rounded-md shadow-sm rendering-pixelated"
                                    />
                                {:else}
                                    <div
                                        class="w-8 h-8 rounded-md bg-muted flex items-center justify-center text-muted-foreground"
                                    >
                                        <Monitor class="w-4 h-4" />
                                    </div>
                                {/if}
                            </Table.Cell>
                            <Table.Cell>
                                <div class="flex flex-col gap-0.5">
                                    <div class="flex items-center gap-2">
                                        <span
                                            class="font-mono text-sm font-semibold tracking-tight"
                                            >{server.ip}:{server.port}</span
                                        >
                                        <Badge
                                            variant={server.server_type ===
                                            "java"
                                                ? "outline"
                                                : "default"}
                                            class="text-[9px] px-1.5 py-0 uppercase italic font-bold"
                                        >
                                            {server.server_type}
                                        </Badge>
                                    </div>
                                    <div
                                        class="text-[11px] text-muted-foreground truncate max-w-[280px] italic"
                                    >
                                        <MinecraftText
                                            text={server.motd ||
                                                "No description available"}
                                        />
                                    </div>
                                </div>
                            </Table.Cell>
                            <Table.Cell>
                                {#if server.status === "online"}
                                    <Badge
                                        class="bg-emerald-500 hover:bg-emerald-600 border-none px-2 py-0 text-[10px] uppercase font-bold tracking-wider"
                                        >Online</Badge
                                    >
                                {:else}
                                    <Badge
                                        variant="outline"
                                        class="text-muted-foreground px-2 py-0 text-[10px] uppercase font-bold tracking-wider"
                                        >Offline</Badge
                                    >
                                {/if}
                            </Table.Cell>
                            <Table.Cell>
                                <div
                                    class="flex items-center gap-1.5 font-mono text-xs"
                                >
                                    <span
                                        class={server.players_online > 0
                                            ? "text-blue-500 font-bold"
                                            : "text-muted-foreground"}
                                        >{server.players_online}</span
                                    >
                                    <span class="text-muted-foreground/50"
                                        >/</span
                                    >
                                    <span class="text-muted-foreground/80"
                                        >{server.players_max}</span
                                    >
                                </div>
                            </Table.Cell>
                            <Table.Cell>
                                <div class="flex flex-col gap-1">
                                    <span
                                        class="text-[10px] font-bold uppercase tracking-widest text-primary/70"
                                        >{server.brand || "Vanilla"}</span
                                    >
                                    <span
                                        class="text-[10px] text-muted-foreground italic truncate max-w-[120px]"
                                        title={server.version}
                                    >
                                        {server.version || "Unknown version"}
                                    </span>
                                </div>
                            </Table.Cell>
                            <Table.Cell class="text-right">
                                <Button
                                    variant="ghost"
                                    size="icon"
                                    class="h-8 w-8 rounded-full opacity-0 group-hover:opacity-100 transition-all"
                                >
                                    <ExternalLink class="h-4 w-4" />
                                </Button>
                            </Table.Cell>
                        </Table.Row>
                    {/each}
                </Table.Body>
            </Table.Root>

            {#if hasMore && servers.length > 0}
                <div class="p-6 flex justify-center border-t">
                    <Button
                        variant="outline"
                        onclick={() => searchServers(true)}
                        disabled={loadingMore}
                        class="w-full max-w-sm italic shadow-sm"
                    >
                        {loadingMore
                            ? "Syncing more records..."
                            : "Load more discovered servers"}
                    </Button>
                </div>
            {/if}
        </Card.Content>
    </Card.Root>
</div>

<style>
    .rendering-pixelated {
        image-rendering: pixelated;
    }
</style>
