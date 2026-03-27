<script lang="ts">
    import { onMount } from "svelte";
    import { browser } from "$app/environment";
    import { env } from "$env/dynamic/public";
    import { Button } from "$lib/components/ui/button";
    import ShieldAlert from "@lucide/svelte/icons/shield-alert";
    import Mail from "@lucide/svelte/icons/mail";
    import MessageCircle from "@lucide/svelte/icons/message-circle";
    import Info from "@lucide/svelte/icons/info";
    import CheckCircle2 from "@lucide/svelte/icons/check-circle-2";
    import Zap from "@lucide/svelte/icons/zap";

    const API_BASE =
        env.PUBLIC_API_URL ||
        (browser ? window.location.origin : "http://localhost:3000");

    let contactInfo = $state({ email: "", discord: "" });

    onMount(async () => {
        try {
            const res = await fetch(`${API_BASE}/api/info`);
            contactInfo = await res.json();
        } catch (e) {
            console.error("Failed to load contact info");
        }
    });
</script>

<div class="min-h-screen bg-background text-foreground flex flex-col">
    <header class="py-20 px-6 text-center border-b">
        <div class="max-w-3xl mx-auto space-y-6">
            <div
                class="inline-flex items-center justify-center w-20 h-20 rounded-2xl bg-primary/10 text-primary mb-2 border border-primary/20 shadow-lg"
            >
                <Zap class="w-10 h-10" />
            </div>
            <h1 class="text-5xl font-extrabold tracking-tight italic">
                NMCScan
            </h1>
            <p class="text-xl text-muted-foreground leading-relaxed italic">
                You reached this page because you noticed a connection from this
                IP address. This is part of an <span
                    class="text-primary font-medium"
                    >Internet Research Project</span
                >
                focused on mapping the global Minecraft ecosystem.
            </p>
        </div>
    </header>

    <main
        class="flex-1 max-w-5xl mx-auto px-6 py-16 grid grid-cols-1 md:grid-cols-2 gap-16"
    >
        <div class="space-y-10">
            <section class="space-y-4">
                <h2 class="text-2xl font-bold flex items-center gap-3">
                    <Info class="w-6 h-6 text-primary" />
                    What is this?
                </h2>
                <p class="text-muted-foreground leading-relaxed">
                    NMCScan is a research system that identifies publicly
                    accessible Minecraft servers. We only perform a standard <span
                        class="italic font-medium">Server List Ping</span
                    >—the same request your Minecraft client sends when you add
                    a server to your multiplayer list.
                </p>
            </section>

            <section
                class="bg-muted/40 border rounded-2xl p-8 space-y-4 shadow-sm"
            >
                <h3 class="text-lg font-bold">Our Commitment</h3>
                <ul class="space-y-4">
                    <li
                        class="flex items-start gap-3 text-sm text-muted-foreground"
                    >
                        <CheckCircle2
                            class="w-5 h-5 text-emerald-500 shrink-0"
                        />
                        No vulnerability exploits or unauthorized access attempts.
                    </li>
                    <li
                        class="flex items-start gap-3 text-sm text-muted-foreground"
                    >
                        <CheckCircle2
                            class="w-5 h-5 text-emerald-500 shrink-0"
                        />
                        Strict rate limiting to ensure no network congestion.
                    </li>
                    <li
                        class="flex items-start gap-3 text-sm text-muted-foreground"
                    >
                        <CheckCircle2
                            class="w-5 h-5 text-emerald-500 shrink-0"
                        />
                        Instant, permanent opt-out upon valid request.
                    </li>
                </ul>
            </section>
        </div>

        <aside
            class="bg-card border rounded-3xl p-8 shadow-xl relative overflow-hidden flex flex-col h-fit"
        >
            <h2 class="text-2xl font-bold mb-4">Request Exclusion</h2>
            <p class="text-muted-foreground text-sm mb-8 leading-relaxed">
                If you wish to have your IP address or network range excluded
                from future scans, please contact us via Discord or Email.
            </p>

            <div class="space-y-4">
                {#if contactInfo.discord}
                    <Button
                        href={contactInfo.discord}
                        target="_blank"
                        variant="outline"
                        class="w-full h-16 justify-start gap-4 text-left border-blue-500/20 bg-blue-500/5 hover:bg-blue-500/10"
                    >
                        <div
                            class="w-10 h-10 rounded-lg bg-[#5865F2] flex items-center justify-center shrink-0"
                        >
                            <MessageCircle class="w-6 h-6 text-white" />
                        </div>
                        <div class="flex-1 overflow-hidden">
                            <div class="font-bold">Join Discord</div>
                            <div
                                class="text-[10px] text-muted-foreground uppercase tracking-widest"
                            >
                                Instant Support
                            </div>
                        </div>
                    </Button>
                {/if}

                {#if contactInfo.email}
                    <Button
                        href={`mailto:${contactInfo.email}`}
                        variant="outline"
                        class="w-full h-16 justify-start gap-4 text-left"
                    >
                        <div
                            class="w-10 h-10 rounded-lg bg-muted flex items-center justify-center shrink-0"
                        >
                            <Mail class="w-6 h-6" />
                        </div>
                        <div class="flex-1 overflow-hidden">
                            <div class="font-bold">Email Us</div>
                            <div class="text-sm truncate text-muted-foreground">
                                {contactInfo.email}
                            </div>
                        </div>
                    </Button>
                {/if}
            </div>

            <div class="mt-12 pt-8 border-t text-center space-y-2">
                <h4
                    class="text-xs font-bold text-muted-foreground uppercase tracking-widest"
                >
                    Research Status
                </h4>
                <p class="text-[11px] italic text-muted-foreground">
                    Exclusion requests are typically processed within 24 hours.
                </p>
            </div>
        </aside>
    </main>

    <footer
        class="py-12 px-6 border-t text-center text-muted-foreground text-xs font-medium"
    >
        &copy; {new Date().getFullYear()} NMCScan Research Project. All rights reserved.
    </footer>
</div>
