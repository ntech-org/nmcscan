<script lang="ts">
    import { page } from "$app/state";
    import { goto } from "$app/navigation";
    import { signOut } from "@auth/sveltekit/client";
    import * as Sidebar from "$lib/components/ui/sidebar";
    import * as Avatar from "$lib/components/ui/avatar";
    import * as DropdownMenu from "$lib/components/ui/dropdown-menu";
    import { Button } from "$lib/components/ui/button";
    import { Badge } from "$lib/components/ui/badge";
    import { Separator } from "$lib/components/ui/separator";
    import { ScrollArea } from "$lib/components/ui/scroll-area";
    import LayoutDashboard from "@lucide/svelte/icons/layout-dashboard";
    import Server from "@lucide/svelte/icons/server";
    import Network from "@lucide/svelte/icons/network";
    import ShieldAlert from "@lucide/svelte/icons/shield-alert";
    import Users from "@lucide/svelte/icons/users";
    import User from "@lucide/svelte/icons/user";
    import ShieldCheck from "@lucide/svelte/icons/shield-check";
    import LogOut from "@lucide/svelte/icons/log-out";
    import ChevronRight from "@lucide/svelte/icons/chevron-right";
    import Search from "@lucide/svelte/icons/search";
    import History from "@lucide/svelte/icons/history";

    let { data, children } = $props();
    const session = $derived(data.session);

    // If session is null and not on the login page, redirect logic can be here
    // But usually it's better to do it in hooks or layout.server.ts
    // For now let's just show login if no session

    const navigation = $derived([
        { title: "Dashboard", href: "/", icon: LayoutDashboard },
        { title: "Online Servers", href: "/admin/servers", icon: Server },
        { title: "ASN Explorer", href: "/admin/asns", icon: Network },
        {
            title: "Exclusion List",
            href: "/admin/exclusions",
            icon: ShieldAlert,
        },
        { title: "Player Lookup", href: "/admin/players", icon: Users },
        { title: "Account Settings", href: "/admin/account", icon: User },
        ...((session?.user as any)?.role === 'admin' ? [{ title: "User Management", href: "/admin/users", icon: ShieldCheck }] : []),
    ]);
</script>

{#if !session && page.url.pathname !== "/login"}
    <div class="flex items-center justify-center min-h-screen">
        <div class="text-center space-y-4">
            <h1 class="text-2xl font-bold">Unauthorized</h1>
            <p class="text-muted-foreground">
                Please sign in to access the dashboard.
            </p>
            <Button href="/login">Go to Login</Button>
        </div>
    </div>
{:else if page.url.pathname === "/login"}
    {@render children()}
{:else}
    <Sidebar.Provider>
        <div class="flex min-h-screen bg-background w-full">
            <Sidebar.Root class="border-r bg-muted/20">
                <Sidebar.Header class="p-4 flex flex-row items-center gap-2">
                    <div
                        class="w-8 h-8 rounded bg-primary flex items-center justify-center text-primary-foreground font-bold italic"
                    >
                        N
                    </div>
                    <span class="font-bold text-lg tracking-tight">NMCScan</span
                    >
                </Sidebar.Header>
                <Sidebar.Content>
                    <Sidebar.Group>
                        <Sidebar.GroupLabel>General</Sidebar.GroupLabel>
                        <Sidebar.GroupContent>
                            <Sidebar.Menu>
                                {#each navigation as item}
                                    <Sidebar.MenuItem>
                                        <Sidebar.MenuButton
                                            isActive={page.url.pathname ===
                                                item.href}
                                            class="gap-3 h-10 px-3"
                                        >
                                            {#snippet child({ props })}
                                                <a href={item.href} {...props}>
                                                    <item.icon size={18} />
                                                    <span class="font-medium"
                                                        >{item.title}</span
                                                    >
                                                </a>
                                            {/snippet}
                                        </Sidebar.MenuButton>
                                    </Sidebar.MenuItem>
                                {/each}
                            </Sidebar.Menu>
                        </Sidebar.GroupContent>
                    </Sidebar.Group>
                </Sidebar.Content>
                <Sidebar.Footer class="p-4 border-t">
                    {#if session?.user}
                        <DropdownMenu.Root>
                            <DropdownMenu.Trigger>
                                {#snippet child({ props })}
                                    <button
                                        {...props}
                                        class="flex items-center gap-3 w-full p-2 rounded-md hover:bg-muted/50 transition-colors"
                                    >
                                        <Avatar.Root class="w-8 h-8">
                                            <Avatar.Image
                                                src={session?.user?.image || undefined}
                                                alt={session?.user?.name || "User"}
                                            />
                                            <Avatar.Fallback
                                                >{session?.user?.name?.charAt(0) ||
                                                    "U"}</Avatar.Fallback
                                            >
                                        </Avatar.Root>
                                        <div
                                            class="flex-1 text-left overflow-hidden"
                                        >
                                            <p
                                                class="text-sm font-medium leading-none truncate"
                                            >
                                                {session?.user?.name || "Unknown"}
                                            </p>
                                            <p
                                                class="text-xs text-muted-foreground leading-tight truncate"
                                            >
                                                {session?.user?.email || ""}
                                            </p>
                                        </div>
                                    </button>
                                {/snippet}
                            </DropdownMenu.Trigger>
                            <DropdownMenu.Content align="end" class="w-56">
                                <DropdownMenu.Label
                                    >My Account</DropdownMenu.Label
                                >
                                <DropdownMenu.Separator />
                                <DropdownMenu.Item onclick={() => goto('/admin/account')} class="gap-2">
                                    <User size={16} />
                                    <span>Account Settings</span>
                                </DropdownMenu.Item>
                                {#if (session?.user as any)?.role === 'admin'}
                                    <DropdownMenu.Item onclick={() => goto('/admin/users')} class="gap-2">
                                        <ShieldCheck size={16} />
                                        <span>User Management</span>
                                    </DropdownMenu.Item>
                                {/if}
                                <DropdownMenu.Separator />
                                <DropdownMenu.Item
                                    onclick={() => signOut()}
                                    class="text-destructive gap-2"
                                >
                                    <LogOut size={16} />
                                    <span>Log out</span>
                                </DropdownMenu.Item>
                            </DropdownMenu.Content>
                        </DropdownMenu.Root>
                    {/if}
                </Sidebar.Footer>
            </Sidebar.Root>

            <main class="flex-1 flex flex-col min-w-0 h-screen overflow-hidden">
                <header
                    class="h-14 border-b bg-background/50 backdrop-blur flex items-center px-6 gap-4 flex-shrink-0"
                >
                    <Sidebar.Trigger class="-ml-2" />
                    <Separator orientation="vertical" class="h-4" />
                    <div class="flex-1">
                        <h1 class="font-semibold text-sm">
                            {#if page.url.pathname === "/"}
                                Dashboard Overview
                            {:else if page.url.pathname.startsWith("/admin/servers")}
                                Online Servers
                            {:else if page.url.pathname.startsWith("/admin/asns")}
                                ASN Database
                            {:else if page.url.pathname.startsWith("/admin/exclusions")}
                                Safety Exclusions
                            {:else if page.url.pathname.startsWith("/admin/players")}
                                Player Search
                            {:else}
                                NMCScan
                            {/if}
                        </h1>
                    </div>
                    <div class="flex items-center gap-2">
                        <Badge
                            variant="outline"
                            class="gap-1 font-mono text-[10px] uppercase tracking-wider"
                        >
                            <span
                                class="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse"
                            ></span>
                            Scanner Active
                        </Badge>
                    </div>
                </header>
                <ScrollArea class="flex-1 p-6">
                    <div class="max-w-7xl mx-auto space-y-6">
                        {@render children()}
                    </div>
                </ScrollArea>
            </main>
        </div>
    </Sidebar.Provider>
{/if}
