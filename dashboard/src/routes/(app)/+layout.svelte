<script lang="ts">
    import { page } from "$app/state";
    import { goto } from "$app/navigation";
    import { signOut } from "@auth/sveltekit/client";
    import * as Avatar from "$lib/components/ui/avatar";
    import * as DropdownMenu from "$lib/components/ui/dropdown-menu";
    import { Badge } from "$lib/components/ui/badge";

    import LayoutDashboard from "@lucide/svelte/icons/layout-dashboard";
    import Server from "@lucide/svelte/icons/server";
    import Network from "@lucide/svelte/icons/network";
    import Users from "@lucide/svelte/icons/users";
    import User from "@lucide/svelte/icons/user";
    import ShieldCheck from "@lucide/svelte/icons/shield-check";
    import Settings from "@lucide/svelte/icons/settings";
    import LogOut from "@lucide/svelte/icons/log-out";

    let { data, children } = $props();
    const session = $derived(data.session);
    const isAdmin = $derived((session?.user as any)?.role === "admin");

    const navItems = $derived([
        { title: "Explore", href: "/explore", icon: LayoutDashboard },
        { title: "Servers", href: "/explore/servers", icon: Server },
        { title: "ASN Explorer", href: "/explore/asns", icon: Network },
        { title: "Players", href: "/explore/players", icon: Users },
    ]);
</script>

<div class="flex min-h-screen bg-background w-full flex-col">
    <!-- Top Navigation Bar -->
    <header class="h-16 border-b bg-background/80 backdrop-blur-md flex items-center px-4 md:px-8 gap-4 flex-shrink-0 sticky top-0 z-50 header-gradient">
        <!-- Logo -->
        <div class="flex items-center gap-3 flex-shrink-0">
            <div
                class="w-9 h-9 rounded-lg bg-primary flex items-center justify-center text-primary-foreground font-bold italic text-lg"
            >
                N
            </div>
            <span class="font-bold text-xl tracking-tight hidden sm:block">NMCScan</span>
        </div>

        <!-- Nav Links -->
        <nav class="flex items-center gap-1 ml-4 md:ml-8">
            {#each navItems as item}
                <a
                    href={item.href}
                    class="flex items-center gap-2 px-3 py-2 rounded-md text-sm font-medium transition-colors
                    {page.url.pathname === item.href || (item.href !== '/explore' && page.url.pathname.startsWith(item.href))
                        ? 'bg-primary/10 text-primary'
                        : 'text-muted-foreground hover:text-foreground hover:bg-muted/50'}"
                >
                    <item.icon size={16} />
                    <span class="hidden md:inline">{item.title}</span>
                </a>
            {/each}
        </nav>

        <div class="flex-1"></div>

        <!-- Right side: Admin link + User menu -->
        <div class="flex items-center gap-3">
            {#if isAdmin}
                <a
                    href="/admin/users"
                    class="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-semibold uppercase tracking-wider
                    text-amber-500 hover:text-amber-400 hover:bg-amber-500/10 transition-colors"
                >
                    <ShieldCheck size={14} />
                    Admin
                </a>
            {/if}

            <DropdownMenu.Root>
                <DropdownMenu.Trigger>
                    {#snippet child({ props })}
                        <button
                            {...props}
                            class="flex items-center gap-2 p-1.5 rounded-full hover:bg-muted/50 transition-colors"
                        >
                            <Avatar.Root class="w-8 h-8">
                                <Avatar.Image
                                    src={session?.user?.image || undefined}
                                    alt={session?.user?.name || "User"}
                                />
                                <Avatar.Fallback
                                    >{session?.user?.name?.charAt(0) || "U"}</Avatar.Fallback
                                >
                            </Avatar.Root>
                        </button>
                    {/snippet}
                </DropdownMenu.Trigger>
                <DropdownMenu.Content align="end" class="w-56">
                    <DropdownMenu.Label class="flex flex-col">
                        <span class="text-sm font-medium">{session?.user?.name || "Unknown"}</span>
                        <span class="text-xs text-muted-foreground truncate">{session?.user?.email || ""}</span>
                    </DropdownMenu.Label>
                    <DropdownMenu.Separator />
                    <DropdownMenu.Item onclick={() => goto("/explore/account")} class="gap-2">
                        <Settings size={16} />
                        <span>Account Settings</span>
                    </DropdownMenu.Item>
                    {#if isAdmin}
                        <DropdownMenu.Item onclick={() => goto("/admin/users")} class="gap-2">
                            <ShieldCheck size={16} />
                            <span>User Management</span>
                        </DropdownMenu.Item>
                    {/if}
                    <DropdownMenu.Separator />
                    <DropdownMenu.Item onclick={() => signOut()} class="text-destructive gap-2">
                        <LogOut size={16} />
                        <span>Log out</span>
                    </DropdownMenu.Item>
                </DropdownMenu.Content>
            </DropdownMenu.Root>
        </div>
    </header>

    <!-- Main Content -->
    <main class="flex-1 w-full fade-in">
        <div class="container-wide py-6 space-y-8">
            {@render children()}
        </div>
    </main>
</div>
