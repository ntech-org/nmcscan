<script lang="ts">
    import { page } from "$app/state";
    import { goto } from "$app/navigation";
    import { signOut } from "@auth/sveltekit/client";
    import * as Sidebar from "$lib/components/ui/sidebar";
    import * as Avatar from "$lib/components/ui/avatar";
    import * as DropdownMenu from "$lib/components/ui/dropdown-menu";
    import { Badge } from "$lib/components/ui/badge";
    import { Separator } from "$lib/components/ui/separator";
    import { ScrollArea } from "$lib/components/ui/scroll-area";

    import ShieldCheck from "@lucide/svelte/icons/shield-check";
    import ShieldAlert from "@lucide/svelte/icons/shield-alert";
    import Scan from "@lucide/svelte/icons/scan";
    import ArrowLeft from "@lucide/svelte/icons/arrow-left";
    import Settings from "@lucide/svelte/icons/settings";
    import LogOut from "@lucide/svelte/icons/log-out";

    let { data, children } = $props();
    const session = $derived(data.session);

    const navigation = [
        { title: "User Management", href: "/admin/users", icon: ShieldCheck },
        { title: "Exclusion List", href: "/admin/exclusions", icon: ShieldAlert },
        { title: "Scanner Control", href: "/admin/scanner", icon: Scan },
    ];
</script>

<Sidebar.Provider>
    <div class="flex min-h-screen bg-background w-full">
        <Sidebar.Root class="border-r bg-muted/20">
            <Sidebar.Header class="p-4 flex flex-col gap-2 border-b">
                <div class="flex items-center gap-2">
                    <div
                        class="w-8 h-8 rounded bg-amber-500/20 flex items-center justify-center text-amber-500 font-bold italic"
                    >
                        A
                    </div>
                    <div>
                        <span class="font-bold text-sm tracking-tight">Admin Panel</span>
                        <p class="text-[10px] text-muted-foreground">NMCScan</p>
                    </div>
                </div>
                <a
                    href="/explore"
                    class="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
                >
                    <ArrowLeft size={12} />
                    Back to dashboard
                </a>
            </Sidebar.Header>
            <Sidebar.Content>
                <Sidebar.Group>
                    <Sidebar.GroupLabel>Administration</Sidebar.GroupLabel>
                    <Sidebar.GroupContent>
                        <Sidebar.Menu>
                            {#each navigation as item}
                                <Sidebar.MenuItem>
                                    <Sidebar.MenuButton
                                        isActive={page.url.pathname === item.href}
                                        class="gap-3 h-10 px-3"
                                    >
                                        {#snippet child({ props })}
                                            <a href={item.href} {...props}>
                                                <item.icon size={18} />
                                                <span class="font-medium">{item.title}</span>
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
                                            >{session?.user?.name?.charAt(0) || "U"}</Avatar.Fallback
                                        >
                                    </Avatar.Root>
                                    <div class="flex-1 text-left overflow-hidden">
                                        <p class="text-sm font-medium leading-none truncate">
                                            {session?.user?.name || "Unknown"}
                                        </p>
                                        <p class="text-xs text-muted-foreground leading-tight truncate">
                                            {session?.user?.email || ""}
                                        </p>
                                    </div>
                                </button>
                            {/snippet}
                        </DropdownMenu.Trigger>
                        <DropdownMenu.Content align="end" class="w-56">
                            <DropdownMenu.Label>My Account</DropdownMenu.Label>
                            <DropdownMenu.Separator />
                            <DropdownMenu.Item onclick={() => goto("/explore/account")} class="gap-2">
                                <Settings size={16} />
                                <span>Account Settings</span>
                            </DropdownMenu.Item>
                            <DropdownMenu.Separator />
                            <DropdownMenu.Item onclick={() => signOut()} class="text-destructive gap-2">
                                <LogOut size={16} />
                                <span>Log out</span>
                            </DropdownMenu.Item>
                        </DropdownMenu.Content>
                    </DropdownMenu.Root>
                {/if}
            </Sidebar.Footer>
        </Sidebar.Root>

        <main class="flex-1 flex flex-col min-w-0 h-screen overflow-hidden">
            <header class="h-14 border-b bg-background/50 backdrop-blur flex items-center px-6 gap-4 flex-shrink-0">
                <Sidebar.Trigger class="-ml-2" />
                <Separator orientation="vertical" class="h-4" />
                <div class="flex-1">
                    <h1 class="font-semibold text-sm">
                        {#if page.url.pathname.startsWith("/admin/users")}
                            User Management
                        {:else if page.url.pathname.startsWith("/admin/exclusions")}
                            Safety Exclusions
                        {:else if page.url.pathname.startsWith("/admin/scanner")}
                            Scanner Control
                        {:else}
                            Admin Panel
                        {/if}
                    </h1>
                </div>
                <Badge variant="outline" class="gap-1.5 font-mono text-[10px] uppercase tracking-wider text-amber-500 border-amber-500/30 bg-amber-500/10">
                    <ShieldCheck size={12} />
                    Admin
                </Badge>
            </header>
            <ScrollArea class="flex-1">
                <div class="max-w-6xl mx-auto p-6 space-y-6 fade-in">
                    {@render children()}
                </div>
            </ScrollArea>
        </main>
    </div>
</Sidebar.Provider>
