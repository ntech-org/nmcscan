<script lang="ts">
  import Landing from "$lib/components/Landing.svelte";

  let { data } = $props();
  const session = $derived(data.session);
</script>

<svelte:head>
  <title>NMCScan - {session ? 'Explore' : 'Research Project'}</title>
</svelte:head>

<Landing />
