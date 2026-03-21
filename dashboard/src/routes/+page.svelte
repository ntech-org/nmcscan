<script lang="ts">
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';

	let contactInfo = $state({ email: '', discord: '' });
	let ip = $state('');
	let comment = $state('');
	let submitting = $state(false);
	let success = $state(false);
	let error = $state('');

	onMount(async () => {
		try {
			const res = await fetch('/info');
			contactInfo = await res.json();
		} catch (e) {
			console.error("Failed to load contact info");
		}
	});

	async function requestExclusion() {
		submitting = true;
		error = '';
		success = false;
		try {
			// Note: This endpoint currently requires API key. 
			// In a real scenario, we might want a public endpoint specifically for single-IP requests
			// or just let them contact via Discord/Email.
			// For now, we will show the contact info as the primary way.
			
			// If we want to allow public single-IP blocking, we'd need another API endpoint.
			// Let's stick to contact info for now to prevent spam/abuse of the exclude API.
			window.location.href = `mailto:${contactInfo.email}?subject=Exclusion Request for ${ip}&body=Please exclude my IP: ${ip}. Reason: ${comment}`;
		} catch (e) {
			error = "Action failed. Please use Discord or Email instead.";
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head>
	<title>NMCScan - Internet Research Project</title>
</svelte:head>

<div class="min-h-screen bg-[#0b0f19] text-gray-200 font-sans selection:bg-blue-500/30 flex flex-col">
	<!-- Hero Section -->
	<header class="py-20 px-4 text-center bg-[radial-gradient(ellipse_at_top,_var(--tw-gradient-stops))] from-blue-900/20 via-[#0b0f19] to-[#0b0f19]">
		<div class="max-w-3xl mx-auto">
			<div class="inline-flex items-center justify-center w-20 h-20 rounded-2xl bg-blue-600/10 text-blue-400 mb-8 border border-blue-500/20 shadow-[0_0_50px_rgba(59,130,246,0.15)]">
				<svg class="w-10 h-10" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"></path></svg>
			</div>
			<h1 class="text-5xl font-extrabold text-white tracking-tight mb-6">NMCScan</h1>
			<p class="text-xl text-gray-400 leading-relaxed">
				You likely reached this page because you noticed an incoming connection from this IP address. 
				This is part of an <span class="text-blue-400 font-medium">Internet Research Project</span> focused on mapping the global Minecraft server ecosystem.
			</p>
		</div>
	</header>

	<!-- Info Grid -->
	<main class="flex-1 max-w-5xl mx-auto px-4 py-12 grid grid-cols-1 md:grid-cols-2 gap-12">
		<!-- Left: About -->
		<div class="space-y-8">
			<div>
				<h2 class="text-2xl font-bold text-white mb-4 flex items-center gap-3">
					<svg class="w-6 h-6 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path></svg>
					What is this?
				</h2>
				<p class="text-gray-400 leading-relaxed">
					NMCScan is a distributed scanning system that identifies publicly accessible Minecraft servers. 
					We only perform a standard <span class="italic">Server List Ping</span>—the same request your Minecraft client sends when you add a server to your multiplayer list.
				</p>
			</div>

			<div class="bg-gray-900/50 border border-gray-800 rounded-2xl p-6 space-y-4">
				<h3 class="text-lg font-semibold text-white">Our Promise</h3>
				<ul class="space-y-3">
					<li class="flex items-start gap-3 text-sm text-gray-400">
						<svg class="w-5 h-5 text-green-500 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path></svg>
						No vulnerability exploits or "hacking" attempts.
					</li>
					<li class="flex items-start gap-3 text-sm text-gray-400">
						<svg class="w-5 h-5 text-green-500 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path></svg>
						Strict rate limiting to avoid network congestion.
					</li>
					<li class="flex items-start gap-3 text-sm text-gray-400">
						<svg class="w-5 h-5 text-green-500 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path></svg>
						Instant opt-out upon request.
					</li>
				</ul>
			</div>
		</div>

		<!-- Right: Contact -->
		<div class="bg-gray-900 border border-gray-800 rounded-3xl p-8 shadow-xl relative overflow-hidden">
			<div class="absolute top-0 right-0 w-32 h-32 bg-blue-500/5 blur-3xl rounded-full -mr-16 -mt-16"></div>
			
			<h2 class="text-2xl font-bold text-white mb-6">Request Exclusion</h2>
			<p class="text-gray-400 text-sm mb-8">
				If you wish to have your IP address or network range excluded from future scans, please contact us through any of the following channels:
			</p>

			<div class="space-y-4">
				{#if contactInfo.discord}
					<a 
						href={contactInfo.discord} 
						target="_blank"
						class="flex items-center gap-4 p-4 bg-[#5865F2]/10 hover:bg-[#5865F2]/20 border border-[#5865F2]/30 rounded-xl transition-all group"
					>
						<div class="w-12 h-12 rounded-lg bg-[#5865F2] flex items-center justify-center shadow-lg shadow-[#5865F2]/20">
							<svg class="w-6 h-6 text-white" fill="currentColor" viewBox="0 0 24 24"><path d="M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037 19.736 19.736 0 0 0-4.885 1.515.069.07 0 0 0-.032.027C.533 9.048-.32 13.58.099 18.057a.082.082 0 0 0 .031.057 19.9 19.9 0 0 0 5.993 3.03.078.078 0 0 0 .084-.028 14.09 14.09 0 0 0 1.226-1.994.076.076 0 0 0-.041-.106 13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128 10.2 10.2 0 0 0 .372-.292.074.074 0 0 1 .077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 0 1 .078.01c.12.098.246.198.373.292a.077.077 0 0 1-.006.127 12.299 12.299 0 0 1-1.873.892.077.077 0 0 0-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.03.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.03zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z"/></svg>
						</div>
						<div>
							<div class="text-white font-bold group-hover:text-blue-400 transition-colors">Join our Discord</div>
							<div class="text-gray-500 text-xs uppercase tracking-widest mt-0.5">Instant Support</div>
						</div>
					</a>
				{/if}

				{#if contactInfo.email}
					<a 
						href={`mailto:${contactInfo.email}`}
						class="flex items-center gap-4 p-4 bg-gray-800/50 hover:bg-gray-800 border border-gray-700 rounded-xl transition-all group"
					>
						<div class="w-12 h-12 rounded-lg bg-gray-700 flex items-center justify-center">
							<svg class="w-6 h-6 text-gray-300" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 8l7.89 5.26a2 2 0 002.22 0L21 8M5 19h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"></path></svg>
						</div>
						<div>
							<div class="text-white font-bold group-hover:text-blue-400 transition-colors">Email Us</div>
							<div class="text-gray-500 text-sm truncate max-w-[200px]">{contactInfo.email}</div>
						</div>
					</a>
				{/if}
			</div>

			<div class="mt-12 pt-8 border-t border-gray-800/50">
				<h4 class="text-sm font-bold text-gray-500 uppercase tracking-widest mb-4 text-center">Project Administrator</h4>
				<div class="text-center italic text-gray-400">
					All exclusion requests are processed within 24-48 hours.
				</div>
			</div>
		</div>
	</main>

	<footer class="py-8 px-4 border-t border-gray-800/30 text-center text-gray-600 text-xs">
		&copy; {new Date().getFullYear()} NMCScan Research. All rights reserved.
	</footer>
</div>
