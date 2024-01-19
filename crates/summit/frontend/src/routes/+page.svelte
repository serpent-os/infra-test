<script lang="ts">
	import { useState } from '$lib';
	import Endpoint from '$lib/component/Endpoint.svelte';

	let app = useState();
</script>

<div>
	<label for="refreshSeconds">Refresh Interval (seconds)</label>
	<input type="number" name="refreshSeconds" bind:value={app.refreshSeconds} min={1} />
</div>
<div>
	<h1>Endpoints</h1>
	<div class="endpoints">
		{#each app.endpoints as endpoint}
			<Endpoint {endpoint} />
		{/each}
	</div>
</div>

<style>
	.endpoints {
		display: flex;
		flex-flow: row wrap;
		gap: 8px;
	}
</style>
