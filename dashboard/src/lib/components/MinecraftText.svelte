<script lang="ts">
    import { parseMinecraftCodes } from '$lib/minecraft';

    let { text = '' } = $props();
    let parts = $derived(parseMinecraftCodes(text || ''));
</script>

<span class="minecraft-text">
    {#each parts as part}
        <span
            style:color={part.color}
            class:font-bold={part.bold}
            class:italic={part.italic}
            class:underline={part.underlined}
            class:line-through={part.strikethrough}
            class:obfuscated={part.obfuscated}
        >
            {part.text}
        </span>
    {/each}
</span>

<style>
    .obfuscated {
        animation: obfuscate 1s infinite alternate;
    }
    @keyframes obfuscate {
        from { opacity: 0.5; }
        to { opacity: 1; filter: blur(1px); }
    }
    .minecraft-text {
        white-space: pre-wrap;
    }
</style>
