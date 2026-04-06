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
    import { Separator } from "$lib/components/ui/separator";

    import Search from "@lucide/svelte/icons/search";
    import RefreshCcw from "@lucide/svelte/icons/refresh-ccw";
    import ExternalLink from "@lucide/svelte/icons/external-link";
    import HardDrive from "@lucide/svelte/icons/hard-drive";
    import Monitor from "@lucide/svelte/icons/monitor";
    import Filter from "@lucide/svelte/icons/filter";

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
        login_obstacle?: string | null;
        last_login_at?: string | null;
        flags?: string[];
    }

    let servers = $state<Server[]>([]);
    let loading = $state(true);
    let loadingMore = $state(false);
    let hasMore = $state(true);
    let error = $state<string | null>(null);
    let showMobileFilters = $state(false);

    // Initial URL Params parsing
    const urlParams = page.url.searchParams;
    let parsedQuery = $state(urlParams.get("search") || "");
    let statusFilter = $state(urlParams.get("status") || "all");
    let serverTypeFilter = $state(urlParams.get("server_type") || "all");
    let loginFilter = $state(urlParams.get("login") || "all");
    let flagFilter = $state(urlParams.get("flags") || "");
    let brandFilter = $state(urlParams.get("brand") || "");
    let versionFilter = $state(urlParams.get("version") || "");
    let countryFilter = $state(urlParams.get("country") || "");
    let minPlayers = $state(urlParams.get("min_players") || "");
    let maxPlayers = $state(urlParams.get("max_players") || "");
    let minMaxPlayers = $state(urlParams.get("min_max_players") || "");
    let maxMaxPlayers = $state(urlParams.get("max_max_players") || "");
    let asnFilter = $state(urlParams.get("asn") || "");
    let asnCategory = $state(urlParams.get("asn_category") || "all");
    
    let sortBy = $state(urlParams.get("sort_by") || "players");
    let sortOrder = $state(urlParams.get("sort_order") || "desc");

    let rawSearchText = $state("");
    let isParsing = false;

    function buildInitialSearchText() {
        isParsing = true;
        let parts = [];
        if (brandFilter) parts.push(`brand:${brandFilter.includes(" ") ? `"${brandFilter}"` : brandFilter}`);
        if (versionFilter) parts.push(`version:${versionFilter.includes(" ") ? `"${versionFilter}"` : versionFilter}`);
        if (countryFilter) parts.push(`country:${countryFilter}`);
        if (statusFilter !== "all") parts.push(`status:${statusFilter}`);
        if (serverTypeFilter !== "all") parts.push(`type:${serverTypeFilter}`);
        if (loginFilter !== "all") parts.push(`login:${loginFilter}`);
        if (flagFilter) {
            flagFilter.split(",").forEach(f => {
                if (f.trim()) parts.push(`flag:${f.trim()}`);
            });
        }
        if (asnCategory !== "all") parts.push(`category:${asnCategory}`);
        if (asnFilter) parts.push(`asn:${asnFilter}`);
        
        // Players Online
        if (minPlayers !== "" && maxPlayers !== "" && minPlayers === maxPlayers) {
            parts.push(`players:${minPlayers}`);
        } else if (minPlayers !== "" && maxPlayers !== "") {
            parts.push(`players:${minPlayers}..${maxPlayers}`);
        } else if (minPlayers !== "") {
            parts.push(`players:>${parseInt(minPlayers) - 1}`);
        } else if (maxPlayers !== "") {
            parts.push(`players:<${parseInt(maxPlayers) + 1}`);
        }

        // Capacity (Max Players)
        if (minMaxPlayers !== "" && maxMaxPlayers !== "" && minMaxPlayers === maxMaxPlayers) {
            parts.push(`limit:${minMaxPlayers}`);
        } else if (minMaxPlayers !== "" && maxMaxPlayers !== "") {
            parts.push(`limit:${minMaxPlayers}..${maxMaxPlayers}`);
        } else if (minMaxPlayers !== "") {
            parts.push(`limit:>${parseInt(minMaxPlayers) - 1}`);
        } else if (maxMaxPlayers !== "") {
            parts.push(`limit:<${parseInt(maxMaxPlayers) + 1}`);
        }

        if (parsedQuery) parts.push(parsedQuery);
        
        rawSearchText = parts.join(" ");
        setTimeout(() => isParsing = false, 50);
    }

    // Parse main search input into discrete filters
    function parseSearchText() {
        isParsing = true;
        const text = rawSearchText;
        const regex = /(?:(brand|version|country|status|type|players|limit|category|asn|login|flag):(?:(["'])(.*?)\2|([^ ]+)))/gi;
        let match;
        let remainingText = text;

        let newBrand = "";
        let newVersion = "";
        let newCountry = "";
        let newStatus = "all";
        let newType = "all";
        let newLogin = "all";
        let newFlags: string[] = [];
        let newMin = "";
        let newMax = "";
        let newMinMax = "";
        let newMaxMax = "";
        let newCat = "all";
        let newAsn = "";

        while ((match = regex.exec(text)) !== null) {
            const fullMatch = match[0];
            const key = match[1].toLowerCase();
            const val = match[3] || match[4];
            
            remainingText = remainingText.replace(fullMatch, "");

            if (key === "brand") newBrand = val;
            if (key === "version") newVersion = val;
            if (key === "country") newCountry = val.toUpperCase();
            if (key === "status") newStatus = ["all", "online", "offline"].includes(val.toLowerCase()) ? val.toLowerCase() : "all";
            if (key === "type") newType = ["all", "java", "bedrock"].includes(val.toLowerCase()) ? val.toLowerCase() : "all";
            if (key === "login") newLogin = val.toLowerCase();
            if (key === "flag") newFlags.push(val.toLowerCase());
            if (key === "category") newCat = val;
            if (key === "asn") newAsn = val;
            if (key === "players") {
                if (val.startsWith(">")) newMin = (parseInt(val.slice(1)) + 1).toString();
                else if (val.startsWith("<")) newMax = (parseInt(val.slice(1)) - 1).toString();
                else if (val.includes("..")) {
                    const parts = val.split("..");
                    newMin = parts[0] || "";
                    newMax = parts[1] || "";
                } else {
                    newMin = val;
                    newMax = val;
                }
            }
            if (key === "limit") {
                if (val.startsWith(">")) newMinMax = (parseInt(val.slice(1)) + 1).toString();
                else if (val.startsWith("<")) newMaxMax = (parseInt(val.slice(1)) - 1).toString();
                else if (val.includes("..")) {
                    const parts = val.split("..");
                    newMinMax = parts[0] || "";
                    newMaxMax = parts[1] || "";
                } else {
                    newMinMax = val;
                    newMaxMax = val;
                }
            }
        }

        brandFilter = newBrand;
        versionFilter = newVersion;
        countryFilter = newCountry;
        statusFilter = newStatus;
        serverTypeFilter = newType;
        loginFilter = newLogin;
        flagFilter = newFlags.join(",");
        asnCategory = newCat;
        asnFilter = newAsn;
        minPlayers = newMin;
        maxPlayers = newMax;
        minMaxPlayers = newMinMax;
        maxMaxPlayers = newMaxMax;
        
        parsedQuery = remainingText.trim().replace(/\s+/g, " ");

        onFilterChange();
        setTimeout(() => isParsing = false, 50);
    }

    // Rebuild text when sidebar filters change
    function buildSearchTextFromSidebar() {
        if (isParsing) return;
        buildInitialSearchText();
        onFilterChange();
    }

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
            if (parsedQuery) params.set("search", parsedQuery);
            if (statusFilter !== "all") params.set("status", statusFilter);
            if (brandFilter) params.set("brand", brandFilter);
            if (versionFilter) params.set("version", versionFilter);
            if (countryFilter) params.set("country", countryFilter);
            if (serverTypeFilter !== "all") params.set("server_type", serverTypeFilter);
            if (loginFilter !== "all") params.set("login", loginFilter);
            if (flagFilter) params.set("flags", flagFilter);
            if (asnCategory !== "all") params.set("asn_category", asnCategory);
            if (asnFilter) params.set("asn", asnFilter);
            if (minPlayers !== "") params.set("min_players", minPlayers);
            if (maxPlayers !== "") params.set("max_players", maxPlayers);
            if (minMaxPlayers !== "") params.set("min_max_players", minMaxPlayers);
            if (maxMaxPlayers !== "") params.set("max_max_players", maxMaxPlayers);
            
            params.set("sort_by", sortBy);
            params.set("sort_order", sortOrder);

            if (!append) {
                goto(`?${params.toString()}`, {
                    replaceState: true,
                    noScroll: true,
                    keepFocus: true,
                });
            } else if (servers.length > 0) {
                const last = servers[servers.length - 1];
                params.set("cursor_ip", last.ip);
                if (sortBy === "players") params.set("cursor_players", last.players_online.toString());
                if (sortBy === "last_seen" && last.last_seen) params.set("cursor_last_seen", last.last_seen);
                
                // Also update URL with cursor when loading more
                goto(`?${params.toString()}`, {
                    replaceState: true,
                    noScroll: true,
                    keepFocus: true,
                });
            }

            const res = await fetchWithAuth(`/api/servers?${params.toString()}`);
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
        buildInitialSearchText();
        searchServers();
    });
</script>

<div class="space-y-6">
    <div class="flex items-center justify-between">
        <div class="space-y-1">
            <h2 class="text-3xl font-bold tracking-tight italic flex items-center gap-3">
                <HardDrive class="text-primary" />
                Server Directory
            </h2>
            <p class="text-muted-foreground italic text-sm">
                Monitor all discovered Minecraft servers across the IPv4 space.
            </p>
        </div>
        <div class="flex items-center gap-2">
            <Button
                variant="outline"
                size="icon"
                onclick={() => showMobileFilters = !showMobileFilters}
                class="md:hidden rounded-full h-10 w-10 shadow-sm"
            >
                <Filter class="h-4 w-4" />
            </Button>
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
    </div>

    <div class="flex flex-col md:flex-row gap-6 items-start">
        <!-- Sidebar Filters -->
        <Card.Root class="w-full md:w-64 flex-shrink-0 bg-card shadow-lg border-muted {showMobileFilters ? 'block' : 'hidden md:block'}">
            <Card.Header class="p-3 pb-2 border-b bg-muted/20">
                <h3 class="font-bold flex items-center gap-2 text-[11px] uppercase tracking-wider text-muted-foreground">
                    <Filter class="h-3.5 w-3.5" /> Filters
                </h3>
            </Card.Header>
            <Card.Content class="p-3 space-y-4">
                <div class="space-y-1.5">
                    <label class="text-[10px] font-semibold uppercase text-muted-foreground/70 ml-1">Status</label>
                    <select
                        bind:value={statusFilter}
                        onchange={buildSearchTextFromSidebar}
                        class="w-full h-8 px-2 bg-background border rounded-md text-xs focus:ring-1 focus:ring-primary outline-none"
                    >
                        <option value="all">All Status</option>
                        <option value="online">Online Only</option>
                        <option value="offline">Offline Only</option>
                    </select>
                </div>

                <div class="space-y-1.5">
                    <label class="text-[10px] font-semibold uppercase text-muted-foreground/70 ml-1">Server Type</label>
                    <select
                        bind:value={serverTypeFilter}
                        onchange={buildSearchTextFromSidebar}
                        class="w-full h-8 px-2 bg-background border rounded-md text-xs focus:ring-1 focus:ring-primary outline-none"
                    >
                        <option value="all">All Types</option>
                        <option value="java">Java (Standard)</option>
                        <option value="bedrock">Bedrock (MCPE)</option>
                    </select>
                </div>

                <div class="space-y-1.5">
                    <label class="text-[10px] font-semibold uppercase text-muted-foreground/70 ml-1">Login Status</label>
                    <select
                        bind:value={loginFilter}
                        onchange={buildSearchTextFromSidebar}
                        class="w-full h-8 px-2 bg-background border rounded-md text-xs focus:ring-1 focus:ring-primary outline-none"
                    >
                        <option value="all">All (Untested)</option>
                        <option value="success">Cracked (Success)</option>
                        <option value="premium">Premium Only</option>
                        <option value="whitelist">Whitelisted</option>
                        <option value="banned">Banned</option>
                        <option value="rejected">Rejected/Failed</option>
                    </select>
                </div>

                <div class="space-y-1.5">
                    <label class="text-[10px] font-semibold uppercase text-muted-foreground/70 ml-1">Quick Tags</label>
                    <div class="flex flex-wrap gap-1.5">
                        {#each ['active', 'modded', 'vanilla', 'cracked', 'premium'] as flag}
                            <button
                                type="button"
                                onclick={() => {
                                    let flags = flagFilter.split(',').filter(f => f.trim());
                                    if (flags.includes(flag)) {
                                        flags = flags.filter(f => f !== flag);
                                    } else {
                                        flags.push(flag);
                                    }
                                    flagFilter = flags.join(',');
                                    buildSearchTextFromSidebar();
                                }}
                                class="px-2 py-0.5 rounded text-[10px] border transition-colors {flagFilter.split(',').includes(flag) ? 'bg-primary/10 border-primary text-primary font-bold' : 'bg-background hover:bg-muted text-muted-foreground'}"
                            >
                                {flag}
                            </button>
                        {/each}
                    </div>
                </div>

                <div class="space-y-1.5">
                    <label class="text-[10px] font-semibold uppercase text-muted-foreground/70 ml-1">All Flags (comma-separated)</label>
                    <Input placeholder="e.g. active,vanilla" class="h-8 text-xs" bind:value={flagFilter} oninput={buildSearchTextFromSidebar} />
                </div>

                <Separator />

                <div class="space-y-1.5">
                    <label class="text-[10px] font-semibold uppercase text-muted-foreground/70 ml-1">Players Online</label>
                    <div class="flex items-center gap-2">
                        <Input type="number" placeholder="Min" class="h-8 text-xs" bind:value={minPlayers} oninput={buildSearchTextFromSidebar} />
                        <span class="text-muted-foreground text-xs">-</span>
                        <Input type="number" placeholder="Max" class="h-8 text-xs" bind:value={maxPlayers} oninput={buildSearchTextFromSidebar} />
                    </div>
                </div>

                <div class="space-y-1.5">
                    <label class="text-[10px] font-semibold uppercase text-muted-foreground/70 ml-1">Server Capacity</label>
                    <div class="flex items-center gap-2">
                        <Input type="number" placeholder="Min" class="h-8 text-xs" bind:value={minMaxPlayers} oninput={buildSearchTextFromSidebar} />
                        <span class="text-muted-foreground text-xs">-</span>
                        <Input type="number" placeholder="Max" class="h-8 text-xs" bind:value={maxMaxPlayers} oninput={buildSearchTextFromSidebar} />
                    </div>
                </div>

                <Separator />

                <div class="space-y-1.5">
                    <label class="text-[10px] font-semibold uppercase text-muted-foreground/70 ml-1">Software</label>
                    <Input placeholder="Brand (e.g. Paper)" class="h-8 text-xs mb-1.5" bind:value={brandFilter} oninput={buildSearchTextFromSidebar} />
                    <Input placeholder="Version (e.g. 1.21)" class="h-8 text-xs" bind:value={versionFilter} oninput={buildSearchTextFromSidebar} />
                </div>

                <Separator />

                <div class="space-y-1.5">
                    <label class="text-[10px] font-semibold uppercase text-muted-foreground/70 ml-1">Network</label>
                    <Input placeholder="Country Code (e.g. US)" class="h-8 text-xs mb-1.5 uppercase" bind:value={countryFilter} oninput={buildSearchTextFromSidebar} maxlength={2} />
                    <Input placeholder="ASN ID (e.g. 16509)" class="h-8 text-xs mb-1.5" bind:value={asnFilter} oninput={buildSearchTextFromSidebar} />
                    <select
                        bind:value={asnCategory}
                        onchange={buildSearchTextFromSidebar}
                        class="w-full h-8 px-2 bg-background border rounded-md text-xs focus:ring-1 focus:ring-primary outline-none"
                    >
                        <option value="all">All Categories</option>
                        <option value="hosting">Hosting</option>
                        <option value="residential">Residential</option>
                        <option value="education">Education</option>
                        <option value="business">Business</option>
                    </select>
                </div>
            </Card.Content>
        </Card.Root>

        <!-- Main Content -->
        <div class="flex-1 min-w-0 w-full space-y-4">
            <Card.Root class="bg-card shadow-lg border-muted !py-0">
                <Card.Header class="p-4 border-b bg-muted/10">
                    <div class="flex flex-col md:flex-row gap-4">
                        <div class="relative flex-1 group">
                            <Search class="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground group-focus-within:text-primary transition-colors" />
                            <Input
                                placeholder='Search... (e.g. version:"1.21" players:>10 country:US)'
                                class="pl-10 h-11 font-mono text-sm bg-background border-muted-foreground/20 focus-visible:ring-primary/50 w-full"
                                bind:value={rawSearchText}
                                oninput={parseSearchText}
                            />
                        </div>
                        <div class="flex-shrink-0">
                            <select
                                bind:value={sortBy}
                                onchange={buildSearchTextFromSidebar}
                                class="h-11 px-3 w-full bg-background border border-muted-foreground/20 rounded-md text-sm font-medium focus:ring-1 focus:ring-primary outline-none"
                            >
                                <option value="players">Sort: Players</option>
                                <option value="last_seen">Sort: Last Seen</option>
                                <option value="ip">Sort: IP Address</option>
                            </select>
                        </div>
                    </div>
                </Card.Header>
                <Card.Content class="p-0 overflow-x-auto">
                    <Table.Root>
                        <Table.Header>
                            <Table.Row class="bg-muted/30 hover:bg-muted/30 uppercase tracking-widest text-[10px] font-bold">
                                <Table.Head class="w-16"></Table.Head>
                                <Table.Head>Server Address</Table.Head>
                                <Table.Head>Status</Table.Head>
                                <Table.Head>Login</Table.Head>
                                <Table.Head>Players</Table.Head>
                                <Table.Head>Software</Table.Head>
                                <Table.Head class="text-right">Action</Table.Head>
                            </Table.Row>
                        </Table.Header>
                        <Table.Body>
                            {#if loading && servers.length === 0}
                                <Table.Row>
                                    <Table.Cell colspan={7} class="h-32 text-center text-muted-foreground">
                                        <div class="flex flex-col items-center justify-center gap-2">
                                            <RefreshCcw class="w-6 h-6 animate-spin mx-auto opacity-50 mb-2" />
                                            <span class="italic text-sm">Querying database...</span>
                                        </div>
                                    </Table.Cell>
                                </Table.Row>
                            {:else if servers.length === 0}
                                <Table.Row>
                                    <Table.Cell colspan={7} class="h-32 text-center text-muted-foreground">
                                        <div class="flex flex-col items-center justify-center gap-2">
                                            <Search class="w-8 h-8 opacity-20" />
                                            <span class="italic text-sm">No servers match your advanced criteria.</span>
                                        </div>
                                    </Table.Cell>
                                </Table.Row>
                            {/if}

                            {#each servers as server}
                                <Table.Row
                                    class="group cursor-pointer hover:bg-muted/30 transition-colors"
                                    onclick={() => goto(`/explore/servers/${server.ip}:${server.port}`)}
                                >
                                    <Table.Cell>
                                        {#if server.favicon}
                                            <img src={server.favicon} alt="" class="w-8 h-8 rounded-md shadow-sm rendering-pixelated" />
                                        {:else}
                                            <div class="w-8 h-8 rounded-md bg-muted/50 border flex items-center justify-center text-muted-foreground">
                                                <Monitor class="w-4 h-4 opacity-50" />
                                            </div>
                                        {/if}
                                    </Table.Cell>
                                    <Table.Cell>
                                        <div class="flex flex-col gap-1">
                                            <div class="flex items-center gap-2">
                                                <span class="font-mono text-sm font-semibold tracking-tight">{server.ip}:{server.port}</span>
                                                {#if server.server_type !== "java"}
                                                    <Badge variant="outline" class="text-[9px] px-1.5 py-0 uppercase italic font-bold">
                                                        {server.server_type}
                                                    </Badge>
                                                {/if}
                                                {#if server.country}
                                                    <span class="text-[10px] text-muted-foreground font-mono bg-muted px-1 rounded">{server.country}</span>
                                                {/if}
                                            </div>
                                            <div class="text-[11px] text-muted-foreground truncate max-w-[200px] md:max-w-[300px] italic">
                                                <MinecraftText text={server.motd || "No description available"} />
                                            </div>
                                        </div>
                                    </Table.Cell>
                                    <Table.Cell>
                                        {#if server.status === "online"}
                                            <Badge class="bg-emerald-500/10 text-emerald-600 hover:bg-emerald-500/20 border-emerald-500/20 px-2 py-0 text-[10px] uppercase font-bold tracking-wider">Online</Badge>
                                        {:else}
                                            <Badge variant="outline" class="text-muted-foreground/70 px-2 py-0 text-[10px] uppercase font-bold tracking-wider">Offline</Badge>
                                        {/if}
                                    </Table.Cell>
                                    <Table.Cell>
                                        {#if server.login_obstacle === 'success'}
                                            <Badge class="bg-emerald-500/10 text-emerald-600 border-emerald-500/20 px-2 py-0 text-[10px] uppercase font-bold tracking-wider">Cracked</Badge>
                                        {:else if server.login_obstacle === 'premium'}
                                            <Badge class="bg-blue-500/10 text-blue-500 border-blue-500/20 px-2 py-0 text-[10px] uppercase font-bold tracking-wider">Premium</Badge>
                                        {:else if server.login_obstacle === 'whitelist'}
                                            <Badge class="bg-yellow-500/10 text-yellow-600 border-yellow-500/20 px-2 py-0 text-[10px] uppercase font-bold tracking-wider">WL</Badge>
                                        {:else if server.login_obstacle === 'banned'}
                                            <Badge class="bg-destructive/10 text-destructive border-destructive/20 px-2 py-0 text-[10px] uppercase font-bold tracking-wider">Banned</Badge>
                                        {:else}
                                            <span class="text-[10px] text-muted-foreground/50">—</span>
                                        {/if}
                                    </Table.Cell>
                                    <Table.Cell>
                                        <div class="flex items-center gap-1.5 font-mono text-xs">
                                            <span class={server.players_online > 0 ? "text-blue-500 dark:text-blue-400 font-bold" : "text-muted-foreground/70"}>
                                                {server.players_online}
                                            </span>
                                            <span class="text-muted-foreground/30">/</span>
                                            <span class="text-muted-foreground/70">{server.players_max}</span>
                                        </div>
                                    </Table.Cell>
                                    <Table.Cell>
                                        <div class="flex flex-col gap-1">
                                            <span class="text-[10px] font-bold uppercase tracking-widest text-primary/70">{server.brand || "Vanilla"}</span>
                                            <span class="text-[10px] text-muted-foreground italic truncate max-w-[120px]" title={server.version || ""}>
                                                {server.version || "Unknown"}
                                            </span>
                                        </div>
                                    </Table.Cell>
                                    <Table.Cell class="text-right pr-4">
                                        <Button variant="ghost" size="icon" class="h-8 w-8 rounded-full opacity-0 group-hover:opacity-100 transition-opacity">
                                            <ExternalLink class="h-4 w-4 text-muted-foreground" />
                                        </Button>
                                    </Table.Cell>
                                </Table.Row>
                            {/each}
                        </Table.Body>
                    </Table.Root>

                    {#if hasMore && servers.length > 0}
                        <div class="p-6 flex justify-center border-t bg-muted/5">
                            <Button
                                variant="outline"
                                onclick={() => searchServers(true)}
                                disabled={loadingMore}
                                class="w-full max-w-sm shadow-sm"
                            >
                                {#if loadingMore}
                                    <RefreshCcw class="w-4 h-4 mr-2 animate-spin" /> Fetching more...
                                {:else}
                                    Load more servers
                                {/if}
                            </Button>
                        </div>
                    {/if}
                </Card.Content>
            </Card.Root>
        </div>
    </div>
</div>

<style>
    .rendering-pixelated {
        image-rendering: pixelated;
    }
</style>
